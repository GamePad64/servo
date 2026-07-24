// surfman/surfman/src/wasm/device.rs
//
//! The null device.
//!
//! A `Device` is surfman's handle to a GPU. There is no GPU inside a wasm guest
//! and no WASI proposal that exposes one, so no `Device` is ever constructed:
//! `Connection::create_device` returns `Error::UnsupportedOnThisPlatform` and
//! every method below is consequently unreachable through the public API. They
//! are implemented as value-returning failures rather than `unimplemented!()`,
//! so a mistaken call degrades instead of aborting the whole component.

use euclid::default::Size2D;
use glow::Texture;
use std::os::raw::c_void;

use crate::device::Device as DeviceInterface;
use crate::wasm::connection::Connection;
use crate::wasm::context::{Context, ContextDescriptor, NativeContext};
use crate::wasm::surface::{NativeWidget, Surface, SurfaceTexture};
use crate::{ContextAttributes, ContextID, Error, GLApi, SurfaceAccess, SurfaceInfo, SurfaceType};

/// A hardware adapter. Inert on this platform.
#[derive(Clone, Debug, Default)]
pub struct Adapter;

/// A native device handle. There is no device to handle.
#[derive(Clone, Debug, Default)]
pub struct NativeDevice;

/// A connection to the GPU. Never successfully created on this platform.
#[derive(Debug)]
pub struct Device;

impl DeviceInterface for Device {
    type Connection = Connection;
    type Context = Context;
    type ContextDescriptor = ContextDescriptor;
    type NativeContext = NativeContext;
    type Surface = Surface;
    type SurfaceTexture = SurfaceTexture;

    fn native_device(&self) -> NativeDevice {
        NativeDevice
    }

    fn connection(&self) -> Connection {
        Connection
    }

    fn adapter(&self) -> Adapter {
        Adapter
    }

    fn gl_api(&self) -> GLApi {
        GLApi::GLES
    }

    fn create_context_descriptor(
        &self,
        attributes: &ContextAttributes,
    ) -> Result<ContextDescriptor, Error> {
        Ok(ContextDescriptor {
            attributes: *attributes,
        })
    }

    fn create_context(
        &self,
        _descriptor: &ContextDescriptor,
        _share_with: Option<&Context>,
    ) -> Result<Context, Error> {
        Err(Error::UnsupportedOnThisPlatform)
    }

    unsafe fn create_context_from_native_context(
        &self,
        _native_context: NativeContext,
    ) -> Result<Context, Error> {
        Err(Error::UnsupportedOnThisPlatform)
    }

    fn destroy_context(&self, _context: &mut Context) -> Result<(), Error> {
        Ok(())
    }

    fn context_descriptor(&self, context: &Context) -> ContextDescriptor {
        let _ = context;
        ContextDescriptor {
            attributes: ContextAttributes::zeroed(),
        }
    }

    fn make_context_current(&self, _context: &Context) -> Result<(), Error> {
        Err(Error::UnsupportedOnThisPlatform)
    }

    fn make_no_context_current(&self) -> Result<(), Error> {
        Ok(())
    }

    fn context_descriptor_attributes(
        &self,
        context_descriptor: &ContextDescriptor,
    ) -> ContextAttributes {
        context_descriptor.attributes
    }

    fn get_proc_address(&self, _context: &Context, _symbol_name: &str) -> *const c_void {
        std::ptr::null()
    }

    fn bind_surface_to_context(
        &self,
        _context: &mut Context,
        _surface: Surface,
    ) -> Result<(), (Error, Surface)> {
        Err((Error::UnsupportedOnThisPlatform, _surface))
    }

    fn unbind_surface_from_context(
        &self,
        _context: &mut Context,
    ) -> Result<Option<Surface>, Error> {
        Ok(None)
    }

    fn context_id(&self, context: &Context) -> ContextID {
        context.id
    }

    fn context_surface_info(&self, _context: &Context) -> Result<Option<SurfaceInfo>, Error> {
        Ok(None)
    }

    fn native_context(&self, _context: &Context) -> NativeContext {
        NativeContext
    }

    fn create_surface(
        &self,
        _context: &Context,
        _surface_access: SurfaceAccess,
        _surface_type: SurfaceType<NativeWidget>,
    ) -> Result<Surface, Error> {
        Err(Error::UnsupportedOnThisPlatform)
    }

    fn create_surface_texture(
        &self,
        _context: &mut Context,
        surface: Surface,
    ) -> Result<SurfaceTexture, (Error, Surface)> {
        Err((Error::UnsupportedOnThisPlatform, surface))
    }

    fn destroy_surface(&self, _context: &mut Context, _surface: &mut Surface) -> Result<(), Error> {
        Ok(())
    }

    fn destroy_surface_texture(
        &self,
        _context: &mut Context,
        surface_texture: SurfaceTexture,
    ) -> Result<Surface, (Error, SurfaceTexture)> {
        Ok(surface_texture.surface)
    }

    fn surface_gl_texture_target(&self) -> u32 {
        0
    }

    fn present_bound_surface(&self, _context: &mut Context) -> Result<(), Error> {
        Err(Error::UnsupportedOnThisPlatform)
    }

    fn present_surface(&self, _context: &Context, _surface: &mut Surface) -> Result<(), Error> {
        Err(Error::UnsupportedOnThisPlatform)
    }

    fn resize_bound_surface(
        &self,
        _context: &mut Context,
        _size: Size2D<i32>,
    ) -> Result<(), Error> {
        Err(Error::UnsupportedOnThisPlatform)
    }

    fn resize_surface(
        &self,
        _context: &Context,
        _surface: &mut Surface,
        _size: Size2D<i32>,
    ) -> Result<(), Error> {
        Err(Error::UnsupportedOnThisPlatform)
    }

    fn surface_info(&self, surface: &Surface) -> SurfaceInfo {
        SurfaceInfo {
            size: surface.size,
            id: crate::SurfaceID(0),
            context_id: surface.context_id,
            framebuffer_object: None,
        }
    }

    fn surface_texture_object(&self, _surface_texture: &SurfaceTexture) -> Option<Texture> {
        None
    }
}
