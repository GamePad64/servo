// Copyright 2015 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Routers allow converting IPC channels to crossbeam channels.
//! The [RouterProxy] provides various methods to register
//! `IpcReceiver<T>`s. The router will then either call the appropriate callback or route the
//! message to a crossbeam `Sender<T>` or `Receiver<T>`. You should use the global `ROUTER` to
//! access the `RouterProxy` methods (via `ROUTER`'s `Deref` for `RouterProxy`.

use std::collections::HashMap;
use std::ops::ControlFlow;
use std::sync::{LazyLock, Mutex};
#[cfg(not(target_family = "wasm"))]
use std::thread::{self, JoinHandle};

use crate::error::{SerDeError, TrySelectError};
use crate::ipc::OpaqueIpcReceiver;
use crate::ipc::{self, IpcMessage, IpcReceiver, IpcReceiverSet, IpcSelectionResult, IpcSender};
use crossbeam_channel::{self, Receiver, Sender};
use serde_core::{Deserialize, Serialize};

/// Global object wrapping a `RouterProxy`.
/// Add routes ([add_typed_route](RouterProxy::add_typed_route)), or convert `IpcReceiver<T>`
/// to crossbeam channels (e.g. [route_ipc_receiver_to_new_crossbeam_receiver](RouterProxy::route_ipc_receiver_to_new_crossbeam_receiver))
pub static ROUTER: LazyLock<RouterProxy> = LazyLock::new(RouterProxy::new);

/// What `RouterProxy` holds to wait for the router loop to finish.
///
/// wasip2 has no threads (`thread::spawn` fails at run time with `os error 58`),
/// so there the loop is a task on the caller's `LocalSet` and there is nothing
/// to join. See [`Router::run_async`].
#[cfg(not(target_family = "wasm"))]
type RouterLoopHandle = JoinHandle<()>;
#[cfg(target_family = "wasm")]
type RouterLoopHandle = ();

/// A `RouterProxy` provides methods for talking to the router. Calling
/// [new](RouterProxy::new) automatically spins up a router thread which
/// waits for events on its registered `IpcReceiver<T>`s. The `RouterProxy`'s
/// methods communicate with the running router thread to register new
/// `IpcReceiver<T>`'s
pub struct RouterProxy {
    comm: Mutex<RouterProxyComm>,
}

impl Drop for RouterProxy {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[allow(clippy::new_without_default)]
impl RouterProxy {
    pub fn new() -> RouterProxy {
        // Router acts like a receiver, running in its own thread with both
        // receiver ends.
        // Router proxy takes both sending ends.
        let (msg_sender, msg_receiver) = crossbeam_channel::unbounded();
        let (wakeup_sender, wakeup_receiver) = ipc::channel().unwrap();
        #[cfg(not(target_family = "wasm"))]
        let handle = thread::Builder::new()
            .name("router-proxy".to_string())
            .spawn(move || Router::new(msg_receiver, wakeup_receiver).run())
            .expect("Failed to spawn router proxy thread");
        #[cfg(target_family = "wasm")]
        let handle = {
            tokio::task::spawn_local(async move {
                Router::new(msg_receiver, wakeup_receiver).run_async().await
            });
        };
        RouterProxy {
            comm: Mutex::new(RouterProxyComm {
                msg_sender,
                wakeup_sender,
                shutdown: false,
                handle: Some(handle),
            }),
        }
    }

    /// Add a new (receiver, callback) pair to the router, and send a wakeup message
    /// to the router.
    ///
    /// The `callback` is dropped when `receiver`'s channel disconnects.
    fn add_route(&self, receiver: OpaqueIpcReceiver, callback: RouterHandler) {
        let comm = self.comm.lock().unwrap();

        if comm.shutdown {
            return;
        }

        comm.msg_sender
            .send(RouterMsg::AddRoute(receiver, callback))
            .unwrap();
        comm.wakeup_sender.send(()).unwrap();
    }

    /// Add a new `(receiver, callback)` pair to the router, and send a wakeup message
    /// to the router.
    ///
    /// The `callback` is dropped when `receiver`'s channel disconnects.
    pub fn add_typed_route<T>(
        &self,
        receiver: IpcReceiver<T>,
        mut callback: TypedRouterMultiHandler<T>,
    ) where
        T: Serialize + for<'de> Deserialize<'de> + 'static,
    {
        // Before passing the message on to the callback, turn it into the appropriate type
        let modified_callback = move |msg: IpcMessage| {
            let typed_message = msg.to::<T>();
            callback(typed_message)
        };

        self.add_route(
            receiver.to_opaque(),
            RouterHandler::Multi(Box::new(modified_callback)),
        );
    }

    /// Add a new `(receiver, callback)` pair to the router, and send a wakeup message
    /// to the router.
    pub fn add_typed_one_shot_route<T>(
        &self,
        receiver: IpcReceiver<T>,
        callback: TypedRouterOneShotHandler<T>,
    ) where
        T: Serialize + for<'de> Deserialize<'de> + 'static,
    {
        // Before passing the message on to the callback, turn it into the appropriate type
        let modified_callback = move |msg: IpcMessage| {
            let typed_message = msg.to::<T>();
            callback(typed_message)
        };

        self.add_route(
            receiver.to_opaque(),
            RouterHandler::Once(Some(Box::new(modified_callback))),
        );
    }

    /// Send a shutdown message to the router containing a ACK sender,
    /// send a wakeup message to the router, and block on the ACK.
    /// Calling it is idempotent,
    /// which can be useful when running a multi-process system in single-process mode.
    pub fn shutdown(&self) {
        let mut comm = self.comm.lock().unwrap();

        if comm.shutdown {
            return;
        }
        comm.shutdown = true;

        // wasip2: the router shares this thread's runtime, so it cannot make
        // progress while we block. Signal it and let it observe that on its next
        // poll, rather than waiting for an ack that could never arrive.
        #[cfg(target_family = "wasm")]
        {
            let (ack_sender, _ack_receiver) = crossbeam_channel::unbounded();
            if comm.wakeup_sender.send(()).is_ok() {
                let _ = comm.msg_sender.send(RouterMsg::Shutdown(ack_sender));
            }
            let _ = comm.handle.take();
            return;
        }

        #[cfg(not(target_family = "wasm"))]
        let (ack_sender, ack_receiver) = crossbeam_channel::unbounded();
        #[cfg(not(target_family = "wasm"))]
        comm.wakeup_sender
            .send(())
            .map(|_| {
                comm.msg_sender
                    .send(RouterMsg::Shutdown(ack_sender))
                    .unwrap();
                ack_receiver.recv().unwrap();
            })
            .unwrap();
        #[cfg(not(target_family = "wasm"))]
        comm.handle
            .take()
            .expect("Should have a join handle at shutdown")
            .join()
            .expect("Failed to join on the router proxy thread");
    }

    /// A convenience function to route an `IpcReceiver<T>` to an existing `Sender<T>`.
    pub fn route_ipc_receiver_to_crossbeam_sender<T>(
        &self,
        ipc_receiver: IpcReceiver<T>,
        crossbeam_sender: Sender<T>,
    ) where
        T: for<'de> Deserialize<'de> + Serialize + Send + 'static,
    {
        self.add_typed_route(
            ipc_receiver,
            Box::new(move |message| drop(crossbeam_sender.send(message.unwrap()))),
        )
    }

    /// A convenience function to route an `IpcReceiver<T>` to a `Receiver<T>`: the most common
    /// use of a `Router`.
    pub fn route_ipc_receiver_to_new_crossbeam_receiver<T>(
        &self,
        ipc_receiver: IpcReceiver<T>,
    ) -> Receiver<T>
    where
        T: for<'de> Deserialize<'de> + Serialize + Send + 'static,
    {
        let (crossbeam_sender, crossbeam_receiver) = crossbeam_channel::unbounded();
        self.route_ipc_receiver_to_crossbeam_sender(ipc_receiver, crossbeam_sender);
        crossbeam_receiver
    }
}

struct RouterProxyComm {
    msg_sender: Sender<RouterMsg>,
    wakeup_sender: IpcSender<()>,
    shutdown: bool,
    handle: Option<RouterLoopHandle>,
}

/// Router runs in its own thread listening for events. Adds events to its IpcReceiverSet
/// and listens for events using select().
struct Router {
    /// Get messages from RouterProxy.
    msg_receiver: Receiver<RouterMsg>,
    /// The ID/index of the special channel we use to identify messages from msg_receiver.
    msg_wakeup_id: u64,
    /// Set of all receivers which have been registered for us to select on.
    ipc_receiver_set: IpcReceiverSet,
    /// Maps ids to their handler functions.
    handlers: HashMap<u64, RouterHandler>,
}

impl Router {
    fn new(msg_receiver: Receiver<RouterMsg>, wakeup_receiver: IpcReceiver<()>) -> Router {
        let mut ipc_receiver_set = IpcReceiverSet::new().unwrap();
        let msg_wakeup_id = ipc_receiver_set.add(wakeup_receiver).unwrap();
        Router {
            msg_receiver,
            msg_wakeup_id,
            ipc_receiver_set,
            handlers: HashMap::new(),
        }
    }

    /// Continuously loop waiting for wakeup signals from router proxy.
    /// Iterate over events either:
    /// 1) If a message comes in from our special `wakeup_receiver` (identified through
    ///    msg_wakeup_id. Read message from `msg_receiver` and add a new receiver
    ///    to our receiver set.
    /// 2) Call appropriate handler based on message id.
    /// 3) Remove handler once channel closes.
    fn run(&mut self) {
        loop {
            // Wait for events to come from our select() new channels are added to
            // our ReceiverSet below.
            let results = match self.ipc_receiver_set.select() {
                Ok(results) => results,
                Err(_) => break,
            };

            if self.handle_results(results) == ControlFlow::Break(()) {
                return;
            }
        }
    }

    /// [`run`](Self::run), but as a task rather than a thread.
    ///
    /// wasip2 has no threads, so the router shares one runtime with every other
    /// Servo actor. A blocking `select()` would stall all of them, so this polls
    /// with `try_select` and yields between passes.
    #[cfg(target_family = "wasm")]
    async fn run_async(&mut self) {
        loop {
            match self.ipc_receiver_set.try_select() {
                Ok(results) => {
                    if self.handle_results(results) == ControlFlow::Break(()) {
                        return;
                    }
                },
                Err(TrySelectError::Empty) => {},
                Err(_) => return,
            }
            tokio::task::yield_now().await;
        }
    }

    /// Dispatch one batch of select results.
    ///
    /// Returns [`ControlFlow::Break`] when the router has been asked to shut down.
    fn handle_results(&mut self, results: Vec<IpcSelectionResult>) -> ControlFlow<()> {
        // Iterate over numerous events that were ready at this time.
        for result in results.into_iter() {
            match result {
                // Message came from the RouterProxy. Listen on our `msg_receiver`
                // channel.
                IpcSelectionResult::MessageReceived(id, _) if id == self.msg_wakeup_id => {
                    match self.msg_receiver.recv().unwrap() {
                        RouterMsg::AddRoute(receiver, handler) => {
                            let new_receiver_id =
                                self.ipc_receiver_set.add_opaque(receiver).unwrap();
                            self.handlers.insert(new_receiver_id, handler);
                        },
                        RouterMsg::Shutdown(sender) => {
                            // The caller may already have stopped waiting for this
                            // ack — on wasip2 it never waits at all.
                            let _ = sender.send(());
                            return ControlFlow::Break(());
                        },
                    }
                },
                // Event from one of our registered receivers, call callback.
                IpcSelectionResult::MessageReceived(id, message) => {
                    match self.handlers.get_mut(&id).unwrap() {
                        RouterHandler::Once(handler) => {
                            if let Some(handler) = handler.take() {
                                (handler)(message);
                            }
                        },
                        RouterHandler::Multi(ref mut handler) => {
                            (handler)(message);
                        },
                    }
                },
                IpcSelectionResult::ChannelClosed(id) => {
                    let _ = self.handlers.remove(&id).unwrap();
                },
            }
        }
        ControlFlow::Continue(())
    }
}

enum RouterMsg {
    /// Register the receiver OpaqueIpcReceiver for listening for events on.
    /// When a message comes from this receiver, call RouterHandler.
    AddRoute(OpaqueIpcReceiver, RouterHandler),
    /// Shutdown the router, providing a sender to send an acknowledgement.
    Shutdown(Sender<()>),
}

/// Function to call when a new event is received from the corresponding receiver.
pub type RouterMultiHandler = Box<dyn FnMut(IpcMessage) + Send>;

/// Function to call the first time that a message is received from the corresponding receiver.
pub type RouterOneShotHandler = Box<dyn FnOnce(IpcMessage) + Send>;

enum RouterHandler {
    Once(Option<RouterOneShotHandler>),
    Multi(RouterMultiHandler),
}

/// Like [RouterMultiHandler] but includes the type that will be passed to the callback
pub type TypedRouterMultiHandler<T> = Box<dyn FnMut(Result<T, SerDeError>) + Send>;

/// Like [RouterOneShotHandler] but includes the type that will be passed to the callback
pub type TypedRouterOneShotHandler<T> = Box<dyn FnOnce(Result<T, SerDeError>) + Send>;
