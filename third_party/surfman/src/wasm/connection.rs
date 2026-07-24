// surfman/surfman/src/wasm/connection.rs
//
//! A connection to a display server that does not exist.
//!
//! There is no display server, GPU or windowing system inside a wasm guest, so a
//! connection carries no state. `new()` succeeds — code that merely holds a
//! connection should not fail — while everything that would require real hardware
//! returns `Error::UnsupportedOnThisPlatform`.

use euclid::default::Size2D;
use std::os::raw::c_void;

use crate::connection::Connection as ConnectionInterface;
use crate::device::Device as DeviceInterface;
use crate::wasm::device::{Adapter, Device, NativeDevice};
use crate::wasm::surface::NativeWidget;
use crate::{Error, GLApi};

/// A connection to the display server. Stateless on wasm.
#[derive(Clone, Debug, Default)]
pub struct Connection;

/// The "native" connection. Nothing underlies it.
#[derive(Clone, Debug, Default)]
pub struct NativeConnection;

impl Connection {
    /// Connects to the default display.
    pub fn new() -> Result<Connection, Error> {
        Ok(Connection)
    }

    /// Returns the underlying native connection.
    pub fn native_connection(&self) -> NativeConnection {
        NativeConnection
    }

    /// Returns the OpenGL API flavor this connection supports.
    ///
    /// Reported as GLES because that is what every non-Apple backend reports; no
    /// context can actually be created, so the value is informational only.
    pub fn gl_api(&self) -> GLApi {
        GLApi::GLES
    }

    /// Returns the "best" adapter. There is no hardware, but an adapter handle
    /// itself is inert, so this succeeds and defers failure to `create_device`.
    pub fn create_adapter(&self) -> Result<Adapter, Error> {
        Ok(Adapter)
    }

    /// Returns the "best" hardware adapter.
    pub fn create_hardware_adapter(&self) -> Result<Adapter, Error> {
        Ok(Adapter)
    }

    /// Returns the "best" low-power adapter.
    pub fn create_low_power_adapter(&self) -> Result<Adapter, Error> {
        Ok(Adapter)
    }

    /// Returns the "best" software adapter.
    pub fn create_software_adapter(&self) -> Result<Adapter, Error> {
        Ok(Adapter)
    }

    /// Opens a device.
    ///
    /// This is where the absence of a GPU becomes unavoidable.
    pub fn create_device(&self, _adapter: &Adapter) -> Result<Device, Error> {
        Err(Error::UnsupportedOnThisPlatform)
    }

    /// Wraps an existing native device.
    ///
    /// # Safety
    ///
    /// No restrictions: there is no native device to misuse.
    pub unsafe fn create_device_from_native_device(
        &self,
        _native_device: NativeDevice,
    ) -> Result<Device, Error> {
        Err(Error::UnsupportedOnThisPlatform)
    }

    /// Creates a native widget from a raw pointer.
    ///
    /// # Safety
    ///
    /// No restrictions: the pointer is never dereferenced.
    pub unsafe fn create_native_widget_from_ptr(
        &self,
        _raw: *mut c_void,
        _size: Size2D<i32>,
    ) -> NativeWidget {
        NativeWidget
    }

    /// Opens a connection from a `RawDisplayHandle`.
    #[cfg(feature = "sm-raw-window-handle-05")]
    pub fn from_raw_display_handle(
        _raw_handle: rwh_05::RawDisplayHandle,
    ) -> Result<Connection, Error> {
        Ok(Connection)
    }

    /// Opens a connection from a `DisplayHandle`.
    #[cfg(feature = "sm-raw-window-handle-06")]
    pub fn from_display_handle(_handle: rwh_06::DisplayHandle) -> Result<Connection, Error> {
        Ok(Connection)
    }

    /// Creates a native widget from a `RawWindowHandle`.
    #[cfg(feature = "sm-raw-window-handle-05")]
    pub fn create_native_widget_from_raw_window_handle(
        &self,
        _window: rwh_05::RawWindowHandle,
        _size: Size2D<i32>,
    ) -> Result<NativeWidget, Error> {
        Err(Error::UnsupportedOnThisPlatform)
    }

    /// Creates a native widget from a `WindowHandle`.
    #[cfg(feature = "sm-raw-window-handle-06")]
    pub fn create_native_widget_from_window_handle(
        &self,
        _window: rwh_06::WindowHandle,
        _size: Size2D<i32>,
    ) -> Result<NativeWidget, Error> {
        Err(Error::UnsupportedOnThisPlatform)
    }
}

impl ConnectionInterface for Connection {
    type Adapter = Adapter;
    type Device = Device;
    type NativeConnection = NativeConnection;
    type NativeDevice = NativeDevice;
    type NativeWidget = NativeWidget;

    fn new() -> Result<Connection, Error> {
        Connection::new()
    }

    fn native_connection(&self) -> Self::NativeConnection {
        Connection::native_connection(self)
    }

    fn gl_api(&self) -> GLApi {
        Connection::gl_api(self)
    }

    fn create_adapter(&self) -> Result<Self::Adapter, Error> {
        Connection::create_adapter(self)
    }

    fn create_hardware_adapter(&self) -> Result<Self::Adapter, Error> {
        Connection::create_hardware_adapter(self)
    }

    fn create_low_power_adapter(&self) -> Result<Self::Adapter, Error> {
        Connection::create_low_power_adapter(self)
    }

    fn create_software_adapter(&self) -> Result<Self::Adapter, Error> {
        Connection::create_software_adapter(self)
    }

    fn create_device(&self, adapter: &Self::Adapter) -> Result<Self::Device, Error> {
        Connection::create_device(self, adapter)
    }

    unsafe fn create_device_from_native_device(
        &self,
        native_device: Self::NativeDevice,
    ) -> Result<Self::Device, Error> {
        unsafe { Connection::create_device_from_native_device(self, native_device) }
    }

    #[cfg(feature = "sm-raw-window-handle-05")]
    fn from_raw_display_handle(raw_handle: rwh_05::RawDisplayHandle) -> Result<Self, Error> {
        Connection::from_raw_display_handle(raw_handle)
    }

    #[cfg(feature = "sm-raw-window-handle-06")]
    fn from_display_handle(handle: rwh_06::DisplayHandle) -> Result<Self, Error> {
        Connection::from_display_handle(handle)
    }

    unsafe fn create_native_widget_from_ptr(
        &self,
        raw: *mut c_void,
        size: Size2D<i32>,
    ) -> Self::NativeWidget {
        unsafe { Connection::create_native_widget_from_ptr(self, raw, size) }
    }

    #[cfg(feature = "sm-raw-window-handle-05")]
    fn create_native_widget_from_raw_window_handle(
        &self,
        window: rwh_05::RawWindowHandle,
        size: Size2D<i32>,
    ) -> Result<Self::NativeWidget, Error> {
        Connection::create_native_widget_from_raw_window_handle(self, window, size)
    }

    #[cfg(feature = "sm-raw-window-handle-06")]
    fn create_native_widget_from_window_handle(
        &self,
        window: rwh_06::WindowHandle,
        size: Size2D<i32>,
    ) -> Result<Self::NativeWidget, Error> {
        Connection::create_native_widget_from_window_handle(self, window, size)
    }
}

// Silence an unused-import warning when no device is ever constructed.
const _: fn() = || {
    fn _assert_device_impls_interface<T: DeviceInterface>() {}
};
