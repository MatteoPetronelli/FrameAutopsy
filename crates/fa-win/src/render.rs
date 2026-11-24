//! WIN-05: the Lab window, its D3D11 device and a flip-model swap chain (`DXGI_SWAP_EFFECT_FLIP_DISCARD`).
//!
//! The public API is safe and small: open, pump messages, draw one frame of a trivial scene (a dark background with a
//! light bar sliding right, so a stall is visible as a jump), present. Every COM call and Win32 call is one `unsafe`
//! operation in its own block. The Lab only ever touches its own window and device.

use std::io;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use windows::core::{w, Interface};
use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11DeviceContext1, ID3D11Texture2D,
    ID3D11View, D3D11_CREATE_DEVICE_FLAG, D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    IDXGIDevice, IDXGIFactory2, IDXGIFactory5, IDXGISwapChain1, DXGI_FEATURE_PRESENT_ALLOW_TEARING,
    DXGI_PRESENT, DXGI_PRESENT_ALLOW_TEARING, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1,
    DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING, DXGI_SWAP_EFFECT_FLIP_DISCARD,
    DXGI_USAGE_RENDER_TARGET_OUTPUT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, LoadCursorW,
    PeekMessageW, RegisterClassExW, TranslateMessage, IDC_ARROW, MSG, PM_REMOVE, WM_CLOSE,
    WNDCLASSEXW, WS_CAPTION, WS_EX_APPWINDOW, WS_SYSMENU, WS_VISIBLE,
};

/// Set by the window procedure when the user closes the window. One Lab window exists at a time.
static CLOSED: AtomicBool = AtomicBool::new(false);

const BAR_WIDTH: i32 = 48;
/// Pixels the bar advances per frame; 8 px per frame is clearly visible at 60 and 144 FPS.
const BAR_STEP: u64 = 8;

/// Left edge of the sliding bar for `frame` in a client area `width` pixels wide (pure, so it is unit tested).
pub fn bar_x(frame: u64, width: i32) -> i32 {
    let span = u64::try_from((width - BAR_WIDTH).max(1)).unwrap_or(1);
    i32::try_from(frame.wrapping_mul(BAR_STEP) % span).unwrap_or(0)
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // The body must not unwind into the OS: a panic falls back to the default procedure's "0".
    catch_unwind(AssertUnwindSafe(|| {
        if msg == WM_CLOSE {
            // The window is destroyed by `Drop`, not here, so the swap chain is never left without a window.
            CLOSED.store(true, Ordering::SeqCst);
            return LRESULT(0);
        }
        // SAFETY: the arguments are exactly those the system passed to this procedure.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }))
    .unwrap_or(LRESULT(0))
}

fn os<E: Into<io::Error>>(e: E) -> io::Error {
    e.into()
}

/// The Lab window with its device, swap chain and render target.
pub struct LabWindow {
    hwnd: HWND,
    context: ID3D11DeviceContext1,
    swap: IDXGISwapChain1,
    view: ID3D11View,
    adapter: String,
    tearing: bool,
    width: i32,
    height: i32,
}

impl LabWindow {
    /// Opens a fixed-size window with a client area of `width` x `height` pixels. The adapter is the system default
    /// (on an Optimus laptop it is not known in advance): read it back with [`adapter_name`](Self::adapter_name).
    pub fn new(width: i32, height: i32) -> io::Result<Self> {
        CLOSED.store(false, Ordering::SeqCst);
        // SAFETY: `None` asks for the handle of the current module; no pointer is involved.
        let module = unsafe { GetModuleHandleW(None) }.map_err(os)?;
        // SAFETY: `None` instance and a predefined cursor id; the returned handle is shared and never freed.
        let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }.map_err(os)?;
        let class = WNDCLASSEXW {
            cbSize: u32::try_from(std::mem::size_of::<WNDCLASSEXW>()).unwrap_or(0),
            lpfnWndProc: Some(wndproc),
            hInstance: module.into(),
            hCursor: cursor,
            lpszClassName: w!("FrameAutopsyLab"),
            ..Default::default()
        };
        // SAFETY: `class` is fully initialised and its class name is a static wide string; registering the same class
        // twice fails harmlessly, which is why a zero result is only an error when the class does not exist yet.
        let atom = unsafe { RegisterClassExW(&class) };
        if atom == 0 && !class_exists(module) {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the class exists, the title is a static wide string, no parent, menu or creation parameter.
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_APPWINDOW,
                w!("FrameAutopsyLab"),
                w!("FrameAutopsy Lab"),
                WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
                100,
                100,
                width + 16,
                height + 39,
                None,
                None,
                Some(module.into()),
                None,
            )
        }
        .map_err(os)?;
        let mut rect = RECT::default();
        // SAFETY: `hwnd` is the live window just created and `rect` is a valid out-pointer.
        unsafe { GetClientRect(hwnd, &mut rect) }.map_err(os)?;
        let (device, context) = create_device()?;
        let (swap, adapter, tearing) = create_swap_chain(&device, hwnd)?;
        let context = context.cast::<ID3D11DeviceContext1>().map_err(os)?;
        let view = create_view(&device, &swap)?;
        Ok(Self {
            hwnd,
            context,
            swap,
            view,
            adapter,
            tearing,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
        })
    }

    /// Description of the adapter the device runs on (for the journal header).
    pub fn adapter_name(&self) -> &str {
        &self.adapter
    }

    /// Dispatches pending window messages. Returns false once the user closed the window.
    pub fn pump(&self) -> bool {
        let mut msg = MSG::default();
        // SAFETY: `msg` is a valid out-pointer; a null window filter reads this thread's own queue.
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            // SAFETY: `msg` was filled by `PeekMessageW` just above.
            let _ = unsafe { TranslateMessage(&msg) };
            // SAFETY: as above; the window procedure is ours and does not unwind.
            unsafe { DispatchMessageW(&msg) };
        }
        !CLOSED.load(Ordering::SeqCst)
    }

    /// Draws frame number `frame`: dark background, light bar at [`bar_x`].
    pub fn draw(&self, frame: u64) {
        let x = bar_x(frame, self.width);
        let bar = RECT {
            left: x,
            top: 0,
            right: x + BAR_WIDTH,
            bottom: self.height,
        };
        // SAFETY: `view` is the live render target of the swap chain; `None` rects clear the whole view.
        unsafe {
            self.context
                .ClearView(&self.view, &[0.05, 0.05, 0.08, 1.0], None)
        };
        // SAFETY: as above; `bar` lies inside the client area and the slice outlives the call.
        unsafe {
            self.context
                .ClearView(&self.view, &[0.95, 0.95, 0.6, 1.0], Some(&[bar]))
        };
    }

    /// True when the system lets a flip-model window present without waiting for the refresh (tearing).
    pub fn tearing_supported(&self) -> bool {
        self.tearing
    }

    /// Presents the frame. `vsync` waits for one vertical blank. Otherwise the present returns at once; with `tear`
    /// (and [`tearing_supported`](Self::tearing_supported)) the frame may replace the one waiting for the refresh,
    /// which is the only way to go above the refresh rate in a window.
    pub fn present(&self, vsync: bool, tear: bool) -> io::Result<()> {
        let flags = if tear && !vsync && self.tearing {
            DXGI_PRESENT_ALLOW_TEARING
        } else {
            DXGI_PRESENT(0)
        };
        // SAFETY: the swap chain is live and owned by `self`; the tearing flag is only set when the swap chain was
        // created with the matching creation flag.
        unsafe { self.swap.Present(u32::from(vsync), flags) }
            .ok()
            .map_err(os)
    }
}

impl Drop for LabWindow {
    fn drop(&mut self) {
        // SAFETY: `hwnd` is the window this object created, destroyed exactly once, here, from the creating thread.
        let _ = unsafe { DestroyWindow(self.hwnd) };
    }
}

fn class_exists(module: HMODULE) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetClassInfoExW;
    let mut info = WNDCLASSEXW {
        cbSize: u32::try_from(std::mem::size_of::<WNDCLASSEXW>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: `info` is a valid out-pointer with `cbSize` set; the class name is a static wide string.
    unsafe { GetClassInfoExW(Some(module.into()), w!("FrameAutopsyLab"), &mut info) }.is_ok()
}

fn create_device() -> io::Result<(ID3D11Device, ID3D11DeviceContext)> {
    let mut device: Option<ID3D11Device> = None;
    let mut context: Option<ID3D11DeviceContext> = None;
    // SAFETY: both out-pointers are valid for the call; no adapter, no software module, no feature-level list.
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_FLAG(0),
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
    }
    .map_err(os)?;
    match (device, context) {
        (Some(d), Some(c)) => Ok((d, c)),
        _ => Err(io::Error::other("D3D11CreateDevice returned no device")),
    }
}

fn create_swap_chain(
    device: &ID3D11Device,
    hwnd: HWND,
) -> io::Result<(IDXGISwapChain1, String, bool)> {
    let dxgi = device.cast::<IDXGIDevice>().map_err(os)?;
    // SAFETY: `dxgi` is a live interface of the device just created.
    let adapter = unsafe { dxgi.GetAdapter() }.map_err(os)?;
    // SAFETY: `adapter` is live; the description is returned by value.
    let desc = unsafe { adapter.GetDesc() }.map_err(os)?;
    let len = desc
        .Description
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(desc.Description.len());
    let name = String::from_utf16_lossy(&desc.Description[..len]);
    // SAFETY: `adapter` is live; its parent is the factory that created it.
    let factory = unsafe { adapter.GetParent::<IDXGIFactory2>() }.map_err(os)?;
    let tearing = tearing_supported(&factory);
    let sc = DXGI_SWAP_CHAIN_DESC1 {
        Width: 0,
        Height: 0,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: 2,
        Scaling: DXGI_SCALING_STRETCH,
        SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
        AlphaMode: DXGI_ALPHA_MODE_IGNORE,
        Flags: if tearing {
            DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING.0 as u32
        } else {
            0
        },
        ..Default::default()
    };
    // SAFETY: `device` and `hwnd` are live, `sc` is fully initialised (zero size = the window's client size).
    let swap =
        unsafe { factory.CreateSwapChainForHwnd(device, hwnd, &sc, None, None) }.map_err(os)?;
    Ok((swap, name, tearing))
}

fn tearing_supported(factory: &IDXGIFactory2) -> bool {
    let Ok(f5) = factory.cast::<IDXGIFactory5>() else {
        return false;
    };
    let mut allowed = windows::core::BOOL(0);
    // SAFETY: `allowed` is a BOOL, the size this feature documents, and outlives the call.
    let ok = unsafe {
        f5.CheckFeatureSupport(
            DXGI_FEATURE_PRESENT_ALLOW_TEARING,
            (&mut allowed as *mut windows::core::BOOL).cast(),
            std::mem::size_of::<windows::core::BOOL>() as u32,
        )
    };
    ok.is_ok() && allowed.as_bool()
}

fn create_view(device: &ID3D11Device, swap: &IDXGISwapChain1) -> io::Result<ID3D11View> {
    // SAFETY: buffer 0 always exists on a swap chain and is the one a flip-model application renders to.
    let buffer = unsafe { swap.GetBuffer::<ID3D11Texture2D>(0) }.map_err(os)?;
    let mut rtv = None;
    // SAFETY: `buffer` is live and `rtv` is a valid out-pointer; no description means the whole texture.
    unsafe { device.CreateRenderTargetView(&buffer, None, Some(&mut rtv)) }.map_err(os)?;
    let rtv = rtv.ok_or_else(|| io::Error::other("CreateRenderTargetView returned no view"))?;
    rtv.cast::<ID3D11View>().map_err(os)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_moves_right_and_wraps_inside_the_client_area() {
        assert_eq!(bar_x(0, 640), 0);
        assert_eq!(bar_x(1, 640), 8);
        for f in 0..2000 {
            let x = bar_x(f, 640);
            assert!((0..=640 - BAR_WIDTH).contains(&x), "frame {f}: {x}");
        }
        assert_eq!(bar_x(5, 1), 0, "a degenerate width must not divide by zero");
    }

    #[test]
    fn a_window_opens_reports_its_adapter_and_presents() {
        let w = LabWindow::new(320, 200).expect("window and device");
        assert!(!w.adapter_name().is_empty());
        for f in 0..3 {
            assert!(w.pump());
            w.draw(f);
            w.present(false, false).unwrap();
        }
    }
}
