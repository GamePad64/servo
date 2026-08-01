/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;

use log::debug;
use malloc_size_of_derive::MallocSizeOf;
use parking_lot::RwLock;
use profile_traits::mem::ReportsChan;
use serde::{Deserialize, Serialize};
use servo_base::generic_channel::{self, GenericSender};
use servo_base::id::PainterId;
use style::values::computed::font::SingleFontFamily;
use webrender_api::units::Au;
use webrender_api::{FontInstanceFlags, FontInstanceKey, FontKey, FontVariation};

use crate::{FontDescriptor, FontIdentifier, FontTemplate, FontTemplateRef};

/// Commands that the `FontContext` sends to the `SystemFontService`.
#[derive(Debug, Deserialize, MallocSizeOf, Serialize)]
pub enum SystemFontServiceMessage {
    GetFontTemplates(
        Option<FontDescriptor>,
        SingleFontFamily,
        GenericSender<Vec<FontTemplate>>,
    ),
    GetFontInstance(
        PainterId,
        FontIdentifier,
        Au,
        FontInstanceFlags,
        Vec<FontVariation>,
        GenericSender<FontInstanceKey>,
    ),
    PrefetchFontKeys(PainterId),
    GetFontKey(PainterId, GenericSender<FontKey>),
    GetFontInstanceKey(PainterId, GenericSender<FontInstanceKey>),
    CollectMemoryReport(ReportsChan),
    Exit(GenericSender<()>),
    Ping,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct SystemFontServiceProxySender(pub GenericSender<SystemFontServiceMessage>);

impl SystemFontServiceProxySender {
    pub fn to_proxy(&self) -> SystemFontServiceProxy {
        SystemFontServiceProxy {
            sender: self.0.clone(),
            templates: Default::default(),
        }
    }
}

#[derive(Debug, Eq, Hash, MallocSizeOf, PartialEq)]
struct FontTemplateCacheKey {
    font_descriptor: Option<FontDescriptor>,
    family_descriptor: SingleFontFamily,
}

/// wasip2: a direct entry point into the running [`SystemFontService`].
///
/// Every Servo actor shares a single thread here, so a proxy round-trip cannot
/// be answered: the service that would answer it is a task, and a task only
/// runs when the caller yields — which layout, synchronous from start to
/// finish, never does. The service therefore registers a direct entry point,
/// and a "send" below becomes a call that has already answered by the time
/// `recv` is reached.
#[cfg(target_family = "wasm")]
thread_local! {
    static INLINE_DISPATCH: std::cell::RefCell<
        Option<std::rc::Rc<dyn Fn(SystemFontServiceMessage)>>,
    > = const { std::cell::RefCell::new(None) };
}

/// Install the inline dispatcher. See [`INLINE_DISPATCH`].
#[cfg(target_family = "wasm")]
pub fn set_inline_dispatch(dispatch: std::rc::Rc<dyn Fn(SystemFontServiceMessage)>) {
    INLINE_DISPATCH.with(|slot| *slot.borrow_mut() = Some(dispatch));
}

/// The public interface to the [`SystemFontService`], used by per-Document
/// `FontContext` instances.
#[derive(Debug, MallocSizeOf)]
pub struct SystemFontServiceProxy {
    sender: GenericSender<SystemFontServiceMessage>,
    templates: RwLock<HashMap<FontTemplateCacheKey, Vec<FontTemplateRef>>>,
}

impl SystemFontServiceProxy {
    /// Deliver a message to the service.
    ///
    /// A channel send, except on wasip2 once the service has registered an
    /// inline dispatcher — see [`INLINE_DISPATCH`].
    fn dispatch(&self, message: SystemFontServiceMessage) -> Result<(), ()> {
        #[cfg(target_family = "wasm")]
        {
            let inline = INLINE_DISPATCH.with(|slot| slot.borrow().clone());
            if let Some(inline) = inline {
                inline(message);
                return Ok(());
            }
        }
        self.sender.send(message).map_err(|_| ())
    }

    pub fn exit(&self) {
        let (response_chan, response_port) = generic_channel::channel().unwrap();
        self.dispatch(SystemFontServiceMessage::Exit(response_chan))
            .expect("Couldn't send SystemFontService exit message");
        response_port
            .recv()
            .expect("Couldn't receive SystemFontService reply");
    }

    pub fn to_sender(&self) -> SystemFontServiceProxySender {
        SystemFontServiceProxySender(self.sender.clone())
    }

    pub fn get_system_font_instance(
        &self,
        identifier: FontIdentifier,
        size: Au,
        flags: FontInstanceFlags,
        variations: Vec<FontVariation>,
        painter_id: PainterId,
    ) -> FontInstanceKey {
        let (response_chan, response_port) =
            generic_channel::channel().expect("failed to create IPC channel");
        self.dispatch(SystemFontServiceMessage::GetFontInstance(
            painter_id,
            identifier,
            size,
            flags,
            variations,
            response_chan,
        ))
        .expect("failed to send message to system font service");

        let instance_key = response_port.recv();
        if instance_key.is_err() {
            let font_thread_has_closed = self.sender.send(SystemFontServiceMessage::Ping).is_err();
            assert!(
                font_thread_has_closed,
                "Failed to receive a response from live font cache"
            );
            panic!("SystemFontService has already exited.");
        }
        instance_key.unwrap()
    }

    pub fn find_matching_font_templates(
        &self,
        descriptor_to_match: Option<&FontDescriptor>,
        family_descriptor: &SingleFontFamily,
    ) -> Vec<FontTemplateRef> {
        let cache_key = FontTemplateCacheKey {
            font_descriptor: descriptor_to_match.cloned(),
            family_descriptor: family_descriptor.clone(),
        };
        if let Some(templates) = self.templates.read().get(&cache_key).cloned() {
            return templates;
        }

        debug!(
            "SystemFontServiceProxy: cache miss for template_descriptor={:?} family_descriptor={:?}",
            descriptor_to_match, family_descriptor
        );

        let (response_chan, response_port) =
            generic_channel::channel().expect("failed to create IPC channel");
        self.dispatch(SystemFontServiceMessage::GetFontTemplates(
            descriptor_to_match.cloned(),
            family_descriptor.clone(),
            response_chan,
        ))
        .expect("failed to send message to system font service");

        let Ok(templates) = response_port.recv() else {
            let font_thread_has_closed = self.sender.send(SystemFontServiceMessage::Ping).is_err();
            assert!(
                font_thread_has_closed,
                "Failed to receive a response from live font cache"
            );
            panic!("SystemFontService has already exited.");
        };

        let templates: Vec<_> = templates.into_iter().map(FontTemplateRef::new).collect();
        self.templates.write().insert(cache_key, templates.clone());

        templates
    }

    pub fn generate_font_key(&self, painter_id: PainterId) -> FontKey {
        let (result_sender, result_receiver) =
            generic_channel::channel().expect("failed to create IPC channel");
        self.dispatch(SystemFontServiceMessage::GetFontKey(
            painter_id,
            result_sender,
        ))
        .expect("failed to send message to system font service");
        result_receiver
            .recv()
            .expect("Failed to communicate with system font service.")
    }

    pub fn generate_font_instance_key(&self, painter_id: PainterId) -> FontInstanceKey {
        let (result_sender, result_receiver) =
            generic_channel::channel().expect("failed to create IPC channel");
        self.dispatch(SystemFontServiceMessage::GetFontInstanceKey(
            painter_id,
            result_sender,
        ))
        .expect("failed to send message to system font service");
        result_receiver
            .recv()
            .expect("Failed to communicate with system font service.")
    }

    pub fn prefetch_font_keys_for_painter(&self, painter_id: PainterId) {
        let _ = self
            .sender
            .send(SystemFontServiceMessage::PrefetchFontKeys(painter_id));
    }
}
