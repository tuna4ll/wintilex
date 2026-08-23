//! The WinTilex status bar.
//!
//! One window per display, drawn with Direct2D and registered with the shell as
//! an appbar so the space it takes comes out of the work area. That is the
//! whole trick behind it fitting into WinTilex without touching the layout
//! engine: tiles are already computed against the work area, so a bar that
//! reserves its strip is simply never drawn under.
//!
//! Like the manager, the bar owns a thread with a message loop, because Win32
//! ties windows to the thread that created them. Everything outside talks to it
//! through [`BarHandle`], which starts and stops that thread and hands it a new
//! configuration.

mod appbar;
mod paint;
mod segments;
mod system;
mod theme;

use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};

use parking_lot::{Mutex, RwLock};

use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Direct2D::ID2D1DCRenderTarget;
use windows::Win32::Graphics::DirectWrite::IDWriteTextFormat;
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, InvalidateRect, HDC, PAINTSTRUCT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, KillTimer,
    LoadCursorW, PeekMessageW, PostThreadMessageW, RegisterClassW, SetLayeredWindowAttributes,
    SetTimer, SetWindowPos, ShowWindow, TranslateMessage, HWND_BOTTOM, HWND_TOPMOST, IDC_ARROW,
    LWA_ALPHA, MA_NOACTIVATE, MSG, PM_NOREMOVE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SW_SHOWNOACTIVATE, WM_APP, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ERASEBKGND, WM_LBUTTONUP,
    WM_MOUSEACTIVATE, WM_PAINT, WM_TIMER, WM_USER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use wintilex_core::command::Action;
use wintilex_core::config::{BarConfig, BarModule, BarPosition};
use wintilex_core::geometry::Rect;
use wintilex_core::manager::EngineHandle;
use wintilex_core::platform::monitor::{enumerate_monitors, MonitorId};
use wintilex_core::platform::window::NativeWindow;

use paint::{HitBoxes, Painter};
use segments::{Act, Readings, Sections};
use system::Cpu;
use theme::Palette;

/// The snapshot changed and every bar needs redrawing.
const WM_BAR_WAKE: u32 = WM_APP + 11;
/// The configuration changed, so the windows are rebuilt from scratch.
const WM_BAR_RELOAD: u32 = WM_APP + 13;
/// Take the bars down and let the thread finish.
const WM_BAR_STOP: u32 = WM_APP + 14;

/// How often the clock and the machine readings are taken.
const TICK_MS: u32 = 1000;

thread_local! {
    /// The bars belonging to this thread. Only ever touched from the window
    /// procedure and the loop below, both of which run on it.
    static BAR: RefCell<Option<Bar>> = const { RefCell::new(None) };
}

struct Shared {
    engine: EngineHandle,
    config: RwLock<BarConfig>,
    /// Zero while no bar thread is running.
    thread_id: AtomicU32,
    /// Held while the thread is being started or stopped, so two calls from
    /// the settings window cannot leave two bars behind.
    lifecycle: Mutex<Option<JoinHandle<()>>>,
}

impl Shared {
    fn post(&self, message: u32) {
        let thread = self.thread_id.load(Ordering::Acquire);
        if thread == 0 {
            return;
        }
        unsafe {
            let _ = PostThreadMessageW(thread, message, WPARAM(0), LPARAM(0));
        }
    }
}

/// Starts, stops and reconfigures the bar. Cheap to clone.
#[derive(Clone)]
pub struct BarHandle {
    inner: Arc<Shared>,
}

impl BarHandle {
    /// Build a handle and subscribe to the manager.
    ///
    /// The subscription is made once and kept for the life of the process:
    /// turning the bar off stops the thread, and with no thread to wake the
    /// callback costs a single atomic read.
    pub fn new(engine: EngineHandle) -> BarHandle {
        let inner = Arc::new(Shared {
            engine: engine.clone(),
            config: RwLock::new(BarConfig::default()),
            thread_id: AtomicU32::new(0),
            lifecycle: Mutex::new(None),
        });

        let listener = Arc::clone(&inner);
        engine.subscribe(Box::new(move || listener.post(WM_BAR_WAKE)));

        BarHandle { inner }
    }

    /// Bring the bar in line with a configuration, starting or stopping the
    /// thread as needed.
    pub fn apply(&self, config: BarConfig) {
        let mut lifecycle = self.inner.lifecycle.lock();
        let wanted = config.enabled;
        *self.inner.config.write() = config;

        match (wanted, lifecycle.is_some()) {
            (true, true) => self.inner.post(WM_BAR_RELOAD),
            (true, false) => *lifecycle = start(Arc::clone(&self.inner)),
            (false, true) => stop(&mut lifecycle, &self.inner),
            (false, false) => {}
        }
    }

    /// Take the bar down. Waits for the thread, so by the time this returns the
    /// reserved space has been handed back to the desktop.
    pub fn shutdown(&self) {
        let mut lifecycle = self.inner.lifecycle.lock();
        if lifecycle.is_some() {
            stop(&mut lifecycle, &self.inner);
        }
    }

    pub fn is_running(&self) -> bool {
        self.inner.thread_id.load(Ordering::Acquire) != 0
    }
}

fn start(shared: Arc<Shared>) -> Option<JoinHandle<()>> {
    let (ready_sender, ready_receiver) = channel();
    let handle = thread::Builder::new()
        .name("wintilex-bar".into())
        .spawn(move || run(shared, ready_sender))
        .map_err(|error| log::error!("could not start the bar thread: {error}"))
        .ok()?;

    // Wait for the queue, otherwise the first reload would be posted into
    // nothing.
    let _ = ready_receiver.recv();
    Some(handle)
}

fn stop(lifecycle: &mut Option<JoinHandle<()>>, shared: &Arc<Shared>) {
    shared.post(WM_BAR_STOP);
    if let Some(handle) = lifecycle.take() {
        let _ = handle.join();
    }
}

fn run(shared: Arc<Shared>, ready: Sender<()>) {
    unsafe {
        let mut probe = MSG::default();
        let _ = PeekMessageW(&mut probe, None, WM_USER, WM_USER, PM_NOREMOVE);
    }
    shared.thread_id.store(unsafe { GetCurrentThreadId() }, Ordering::Release);
    let _ = ready.send(());

    if let Some(painter) = Painter::new() {
        let mut bar = Bar::new(Arc::clone(&shared), painter);
        bar.rebuild();
        BAR.with(|cell| *cell.borrow_mut() = Some(bar));

        let tick = unsafe { SetTimer(None, 0, TICK_MS, None) };
        pump();
        unsafe {
            let _ = KillTimer(None, tick);
        }

        // Taken out of the cell before it is dropped: closing the windows sends
        // messages straight back into the window procedure, which must not find
        // the bar half torn down.
        let bar = BAR.with(|cell| cell.borrow_mut().take());
        drop(bar);
    }

    shared.thread_id.store(0, Ordering::Release);
    // The strip the bar was using is free again, so put the tiles back over it.
    shared.engine.dispatch(Action::Retile);
    log::info!("bar thread stopped");
}

fn pump() {
    let mut message = MSG::default();
    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 {
            return;
        }

        // Thread messages arrive without a window and never need dispatching.
        match message.message {
            WM_BAR_STOP => return,
            WM_BAR_WAKE => with_bar(|bar| bar.invalidate()),
            WM_BAR_RELOAD => with_bar(|bar| bar.rebuild()),
            WM_TIMER if message.hwnd.is_invalid() => with_bar(|bar| bar.tick()),
            _ => unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            },
        }
    }
}

/// Run something against the bars on this thread.
///
/// Re-entrancy is real here: painting a window can end up back inside the
/// window procedure. Anything that arrives while the bar is already borrowed is
/// dropped rather than allowed to panic.
fn with_bar(action: impl FnOnce(&mut Bar)) {
    BAR.with(|cell| {
        if let Ok(mut borrowed) = cell.try_borrow_mut() {
            if let Some(bar) = borrowed.as_mut() {
                action(bar);
            }
        }
    });
}

/// One bar window, and the drawing state that belongs to it.
struct BarWindow {
    hwnd: HWND,
    monitor: MonitorId,
    /// What the display is called on the bar: 1 for the primary, then across.
    number: usize,
    scale: f32,
    rect: Rect,
    target: Option<ID2D1DCRenderTarget>,
    hits: HitBoxes,
    /// Whether the shell took the appbar registration.
    reserved: bool,
}

impl Drop for BarWindow {
    fn drop(&mut self) {
        log::debug!("closing the bar on {}", self.monitor);
        if self.reserved {
            appbar::unregister(self.hwnd);
        }
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

struct Bar {
    shared: Arc<Shared>,
    painter: Painter,
    config: BarConfig,
    palette: Palette,
    fonts: Option<(IDWriteTextFormat, IDWriteTextFormat)>,
    windows: Vec<BarWindow>,
    cpu: Cpu,
    readings: Readings,
    clock: String,
}

impl Bar {
    fn new(shared: Arc<Shared>, painter: Painter) -> Bar {
        let config = shared.config.read().clone();
        Bar {
            shared,
            painter,
            palette: Palette::from_theme(&config.theme),
            fonts: None,
            config,
            windows: Vec::new(),
            cpu: Cpu::default(),
            readings: Readings::default(),
            clock: String::new(),
        }
    }

    /// Throw the windows away and put them back, which is what a new
    /// configuration or a change of displays comes down to.
    fn rebuild(&mut self) {
        self.config = self.shared.config.read().clone();
        self.palette = Palette::from_theme(&self.config.theme);
        self.fonts = self.build_fonts();

        // Dropping the old windows hands back their appbar slots first.
        self.windows.clear();

        let Ok(instance) = (unsafe { GetModuleHandleW(None) }) else {
            return;
        };
        register_class();

        for (index, monitor) in enumerate_monitors().into_iter().enumerate() {
            if self.config.primary_only && !monitor.is_primary {
                continue;
            }

            let scale = monitor.scale();
            let height = self.config.scaled_height(scale);
            let opacity = self.config.window_opacity();

            let mut style = WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST;
            if opacity < 1.0 {
                style |= WS_EX_LAYERED;
            }

            let proposed = strip(monitor.bounds, self.config.position, height);
            let hwnd = unsafe {
                CreateWindowExW(
                    style,
                    CLASS_NAME,
                    w!("WinTilex bar"),
                    WS_POPUP,
                    proposed.x,
                    proposed.y,
                    proposed.width,
                    proposed.height,
                    None,
                    None,
                    Some(instance.into()),
                    None,
                )
            };
            let Ok(hwnd) = hwnd else {
                log::error!("could not create the bar window for {}", monitor.id);
                continue;
            };

            if opacity < 1.0 {
                unsafe {
                    let _ = SetLayeredWindowAttributes(
                        hwnd,
                        COLORREF(0),
                        (opacity * 255.0) as u8,
                        LWA_ALPHA,
                    );
                }
            }

            // With the space reserved the shell decides where the strip ends
            // up, which is how the bar stays clear of a taskbar on the same
            // edge. Without it the bar floats inside the work area instead.
            let reserved = self.config.reserve_space && appbar::register(hwnd);
            let rect = if reserved {
                appbar::place(hwnd, self.config.position, proposed)
            } else {
                strip(monitor.work_area, self.config.position, height)
            };

            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height,
                    SWP_NOACTIVATE,
                );
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }

            log::debug!("bar on {} at {rect:?}, space reserved: {reserved}", monitor.id);
            self.windows.push(BarWindow {
                hwnd,
                monitor: monitor.id.clone(),
                number: index + 1,
                scale,
                rect,
                target: None,
                hits: Vec::new(),
                reserved,
            });
        }

        log::info!("bar showing on {} display(s)", self.windows.len());
        self.tick();
        // The work area just moved under the manager's feet.
        self.shared.engine.dispatch(Action::Retile);
    }

    fn build_fonts(&self) -> Option<(IDWriteTextFormat, IDWriteTextFormat)> {
        let family = &self.config.font_family;
        let size = self.config.font_size;
        Some((
            self.painter.text_format(family, size, false)?,
            self.painter.text_format(family, size, true)?,
        ))
    }

    /// Re-read the clock and the machine, and redraw if anything moved.
    fn tick(&mut self) {
        let clock = if self.uses(BarModule::Clock) {
            system::clock(&self.config.clock_format)
        } else {
            String::new()
        };

        let readings = Readings {
            cpu: if self.uses(BarModule::Cpu) { self.cpu.sample() } else { 0 },
            memory: if self.uses(BarModule::Memory) { system::memory_load() } else { 0 },
            battery: self.uses(BarModule::Battery).then(system::battery).flatten(),
        };

        // A clock showing hours and minutes changes twice an hour, so the bar
        // sits still between them instead of repainting every second.
        if clock != self.clock || readings != self.readings {
            self.clock = clock;
            self.readings = readings;
            self.invalidate();
        }
    }

    fn uses(&self, module: BarModule) -> bool {
        let sections = [&self.config.left, &self.config.center, &self.config.right];
        sections.iter().any(|section| section.contains(&module))
    }

    fn invalidate(&self) {
        for window in &self.windows {
            unsafe {
                let _ = InvalidateRect(Some(window.hwnd), None, false);
            }
        }
    }

    fn index_of(&self, hwnd: HWND) -> Option<usize> {
        self.windows.iter().position(|window| window.hwnd == hwnd)
    }

    fn paint(&mut self, hwnd: HWND, hdc: HDC) {
        let Some(index) = self.index_of(hwnd) else {
            return;
        };
        let Some(fonts) = self.fonts.as_ref() else {
            return;
        };

        let size = (self.windows[index].rect.width, self.windows[index].rect.height);
        if self.windows[index].target.is_none() {
            self.windows[index].target = self.painter.target();
        }
        let Some(target) = self.windows[index].target.clone() else {
            return;
        };

        let snapshot = self.shared.engine.snapshot();
        let window = &self.windows[index];
        let sections = snapshot
            .monitors
            .iter()
            .find(|view| view.id == window.monitor)
            .map(|view| {
                segments::build(
                    &snapshot,
                    &self.config,
                    view,
                    window.number,
                    &self.readings,
                    &self.clock,
                )
            })
            .unwrap_or_else(Sections::default);

        let painted = paint::draw(
            &self.painter,
            &target,
            (hdc, size),
            (&fonts.0, &fonts.1),
            &self.palette,
            &sections,
            window.scale,
        );

        self.windows[index].hits = painted.hits;
        if painted.device_lost {
            // Dropping the target is the only way back: the next paint builds
            // a new one against whatever device is there now.
            self.windows[index].target = None;
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
        }
    }

    fn click(&mut self, hwnd: HWND, x: i32, y: i32) {
        let Some(index) = self.index_of(hwnd) else {
            return;
        };
        let hit = self.windows[index]
            .hits
            .iter()
            .find(|(rect, _)| contains(*rect, x, y))
            .map(|(_, action)| *action);

        match hit {
            Some(Act::Focus(id)) => {
                let window = NativeWindow::from_id(id);
                if window.is_minimized() {
                    window.restore();
                }
                window.focus();
            }
            Some(Act::CycleLayout) => {
                self.reach_display(index);
                self.shared.engine.dispatch(Action::CycleLayout);
            }
            Some(Act::ToggleTiling) => self.shared.engine.dispatch(Action::ToggleTiling),
            None => {}
        }
    }

    /// Put the focus on the display this bar belongs to.
    ///
    /// Layout actions land on whichever display holds the focused window, so
    /// clicking the layout on the second screen would otherwise cycle the
    /// first one. Nothing happens when the focus is already over here.
    fn reach_display(&self, index: usize) {
        let snapshot = self.shared.engine.snapshot();
        let monitor = &self.windows[index].monitor;

        let elsewhere = snapshot
            .focused
            .and_then(|id| snapshot.windows.iter().find(|window| window.id == id))
            .is_none_or(|window| window.monitor != *monitor);
        if !elsewhere {
            return;
        }

        let first = snapshot
            .monitors
            .iter()
            .find(|view| view.id == *monitor)
            .and_then(|view| view.order.first().copied());
        if let Some(id) = first {
            NativeWindow::from_id(id).focus();
        }
    }

    /// Answer the shell. The one that matters is `ABN_FULLSCREENAPP`: a bar
    /// left on top of a game or a full-screen video would be a bug report.
    fn appbar_message(&mut self, hwnd: HWND, notification: usize, flag: isize) {
        match notification {
            appbar::ABN_FULLSCREENAPP => {
                let behind = flag != 0;
                let after = if behind { HWND_BOTTOM } else { HWND_TOPMOST };
                unsafe {
                    let _ = SetWindowPos(
                        hwnd,
                        Some(after),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
            }
            appbar::ABN_POSCHANGED | appbar::ABN_STATECHANGE => self.reposition(hwnd),
            _ => {}
        }
    }

    /// Another appbar moved, so ask for the strip again and take what is left.
    fn reposition(&mut self, hwnd: HWND) {
        let Some(index) = self.index_of(hwnd) else {
            return;
        };
        if !self.windows[index].reserved {
            return;
        }

        let rect = appbar::place(hwnd, self.config.position, self.windows[index].rect);
        if rect == self.windows[index].rect {
            return;
        }

        unsafe {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                SWP_NOACTIVATE,
            );
        }
        appbar::moved(hwnd);

        self.windows[index].rect = rect;
        self.windows[index].target = None;
    }
}

/// The strip of `area` an edge-aligned bar of this height would take.
fn strip(area: Rect, position: BarPosition, height: i32) -> Rect {
    match position {
        BarPosition::Top => Rect::new(area.x, area.y, area.width, height),
        BarPosition::Bottom => Rect::new(area.x, area.y + area.height - height, area.width, height),
    }
}

fn contains(rect: Rect, x: i32, y: i32) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

const CLASS_NAME: windows::core::PCWSTR = w!("WinTilexBar");

fn register_class() {
    static REGISTERED: OnceLock<()> = OnceLock::new();
    REGISTERED.get_or_init(|| {
        let instance = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: CLASS_NAME,
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
            ..Default::default()
        };
        unsafe { RegisterClassW(&class) };
    });
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        // Clicking the bar must never take the focus away from the window the
        // bar is describing.
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            // `BeginPaint` is what hands over the device context Direct2D draws
            // onto, so the whole frame happens between it and `EndPaint`.
            let mut paint = PAINTSTRUCT::default();
            unsafe {
                BeginPaint(hwnd, &mut paint);
            }
            with_bar(|bar| bar.paint(hwnd, paint.hdc));
            unsafe {
                let _ = EndPaint(hwnd, &paint);
            }
            return LRESULT(0);
        }
        WM_LBUTTONUP => {
            let (x, y) = (loword(lparam.0), hiword(lparam.0));
            with_bar(|bar| bar.click(hwnd, x, y));
            return LRESULT(0);
        }
        appbar::WM_APPBAR => {
            with_bar(|bar| bar.appbar_message(hwnd, wparam.0, lparam.0));
            return LRESULT(0);
        }
        // Rebuilding here would destroy the very window whose procedure this
        // is, so the work is handed back to the loop.
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            with_bar(|bar| bar.shared.post(WM_BAR_RELOAD));
            return LRESULT(0);
        }
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn loword(value: isize) -> i32 {
    (value & 0xffff) as i16 as i32
}

fn hiword(value: isize) -> i32 {
    ((value >> 16) & 0xffff) as i16 as i32
}
