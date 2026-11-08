//! Touch handling.

use std::time::Duration;

use tracing::warn;

use sctk::compositor::SurfaceData;
use sctk::reexports::client::protocol::wl_seat::WlSeat;
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::protocol::wl_touch::WlTouch;
use sctk::reexports::client::{Connection, Proxy, QueueHandle};

use sctk::seat::touch::{TouchData, TouchHandler};

use crate::dpi::LogicalPosition;
use crate::event::{Touch, TouchPhase, WindowEvent};
use crate::frame_input::FrameInput;

use crate::platform_impl::wayland::state::WinitState;
use crate::platform_impl::wayland::{self, DeviceId};

impl TouchHandler for WinitState {
    fn down(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        touch: &WlTouch,
        serial: u32,
        time: u32,
        surface: WlSurface,
        id: i32,
        position: (f64, f64),
    ) {
        // TraceDraw: a finger on the window's frame drives the frame, as
        // the mouse does (#2070); before, the touch found no window and
        // was dropped.
        if frame_parent(&surface).is_some() {
            let seat = touch.seat().clone();
            let input = FrameInput::FingerDown(position.0, position.1);
            self.frame_touch(&seat, &surface, input, serial, time);
            if let Some(seat_state) = self.seats.get_mut(&seat.id()) {
                let location = LogicalPosition::<f64>::from(position);
                seat_state.touch_map.insert(id, TouchPoint { surface, location });
            }
            return;
        }

        let window_id = wayland::make_wid(&surface);
        let scale_factor = match self.windows.get_mut().get(&window_id) {
            Some(window) => window.lock().unwrap().scale_factor(),
            None => return,
        };

        let seat_state = match self.seats.get_mut(&touch.seat().id()) {
            Some(seat_state) => seat_state,
            None => {
                warn!("Received wl_touch::down without seat");
                return;
            },
        };

        // Update the state of the point.
        let location = LogicalPosition::<f64>::from(position);
        seat_state.touch_map.insert(id, TouchPoint { surface, location });

        self.events_sink.push_window_event(
            WindowEvent::Touch(Touch {
                device_id: crate::event::DeviceId(crate::platform_impl::DeviceId::Wayland(
                    DeviceId,
                )),
                phase: TouchPhase::Started,
                location: location.to_physical(scale_factor),
                force: None,
                id: id as u64,
            }),
            window_id,
        );
    }

    fn up(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        touch: &WlTouch,
        serial: u32,
        time: u32,
        id: i32,
    ) {
        let seat_state = match self.seats.get_mut(&touch.seat().id()) {
            Some(seat_state) => seat_state,
            None => {
                warn!("Received wl_touch::up without seat");
                return;
            },
        };

        // Remove the touch point.
        let touch_point = match seat_state.touch_map.remove(&id) {
            Some(touch_point) => touch_point,
            None => return,
        };

        // TraceDraw: a finger lifted from the window's frame (#2070).
        if frame_parent(&touch_point.surface).is_some() {
            let seat = touch.seat().clone();
            self.frame_touch(&seat, &touch_point.surface, FrameInput::FingerUp, serial, time);
            return;
        }

        let window_id = wayland::make_wid(&touch_point.surface);
        let scale_factor = match self.windows.get_mut().get(&window_id) {
            Some(window) => window.lock().unwrap().scale_factor(),
            None => return,
        };

        self.events_sink.push_window_event(
            WindowEvent::Touch(Touch {
                device_id: crate::event::DeviceId(crate::platform_impl::DeviceId::Wayland(
                    DeviceId,
                )),
                phase: TouchPhase::Ended,
                location: touch_point.location.to_physical(scale_factor),
                force: None,
                id: id as u64,
            }),
            window_id,
        );
    }

    fn motion(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        touch: &WlTouch,
        time: u32,
        id: i32,
        position: (f64, f64),
    ) {
        let seat_state = match self.seats.get_mut(&touch.seat().id()) {
            Some(seat_state) => seat_state,
            None => {
                warn!("Received wl_touch::motion without seat");
                return;
            },
        };

        // Remove the touch point.
        let touch_point = match seat_state.touch_map.get_mut(&id) {
            Some(touch_point) => touch_point,
            None => return,
        };

        // TraceDraw: a finger moving on the window's frame starts the move
        // or the resize its press asked for, with the press's serial the
        // window kept (#2070).
        if frame_parent(&touch_point.surface).is_some() {
            touch_point.location = LogicalPosition::<f64>::from(position);
            let surface = touch_point.surface.clone();
            let seat = touch.seat().clone();
            let input = FrameInput::Moved(position.0, position.1);
            self.frame_touch(&seat, &surface, input, 0, time);
            return;
        }

        let window_id = wayland::make_wid(&touch_point.surface);
        let scale_factor = match self.windows.get_mut().get(&window_id) {
            Some(window) => window.lock().unwrap().scale_factor(),
            None => return,
        };

        touch_point.location = LogicalPosition::<f64>::from(position);

        self.events_sink.push_window_event(
            WindowEvent::Touch(Touch {
                device_id: crate::event::DeviceId(crate::platform_impl::DeviceId::Wayland(
                    DeviceId,
                )),
                phase: TouchPhase::Moved,
                location: touch_point.location.to_physical(scale_factor),
                force: None,
                id: id as u64,
            }),
            window_id,
        );
    }

    fn cancel(&mut self, _: &Connection, _: &QueueHandle<Self>, touch: &WlTouch) {
        let seat_state = match self.seats.get_mut(&touch.seat().id()) {
            Some(seat_state) => seat_state,
            None => {
                warn!("Received wl_touch::cancel without seat");
                return;
            },
        };

        // TraceDraw: the points are taken first, so that the frame under
        // a finger can be left (#2070).
        let points: Vec<_> = seat_state.touch_map.drain().collect();
        for (id, touch_point) in points {
            if frame_parent(&touch_point.surface).is_some() {
                let seat = touch.seat().clone();
                self.frame_touch(&seat, &touch_point.surface, FrameInput::Left, 0, 0);
                continue;
            }
            let window_id = wayland::make_wid(&touch_point.surface);
            let scale_factor = match self.windows.get_mut().get(&window_id) {
                Some(window) => window.lock().unwrap().scale_factor(),
                None => return,
            };

            let location = touch_point.location.to_physical(scale_factor);

            self.events_sink.push_window_event(
                WindowEvent::Touch(Touch {
                    device_id: crate::event::DeviceId(crate::platform_impl::DeviceId::Wayland(
                        DeviceId,
                    )),
                    phase: TouchPhase::Cancelled,
                    location,
                    force: None,
                    id: id as u64,
                }),
                window_id,
            );
        }
    }

    fn shape(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlTouch,
        _: i32,
        _: f64,
        _: f64,
    ) {
        // Blank.
    }

    fn orientation(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlTouch, _: i32, _: f64) {
        // Blank.
    }
}

impl WinitState {
    /// TraceDraw: tell the frame `surface` belongs to what a finger did on
    /// it (#2070); `serial` is the press's, `time` the event's.
    fn frame_touch(
        &mut self,
        seat: &WlSeat,
        surface: &WlSurface,
        input: FrameInput,
        serial: u32,
        time: u32,
    ) {
        let Some(parent) = frame_parent(surface) else {
            return;
        };
        let window_id = wayland::make_wid(&parent);
        let Some(window) = self.windows.get_mut().get(&window_id).cloned() else {
            return;
        };
        window.lock().unwrap().frame_input(
            input,
            seat,
            surface,
            serial,
            Duration::from_millis(time as u64),
            window_id,
            &mut self.window_compositor_updates,
        );
    }
}

/// TraceDraw: the window's surface for a surface of its frame (one with
/// a parent); `None` for the window's own surface (#2070).
fn frame_parent(surface: &WlSurface) -> Option<WlSurface> {
    surface.data::<SurfaceData>()?.parent_surface().cloned()
}

/// The state of the touch point.
#[derive(Debug)]
pub struct TouchPoint {
    /// The surface on which the point is present.
    pub surface: WlSurface,

    /// The location of the point on the surface.
    pub location: LogicalPosition<f64>,
}

pub trait TouchDataExt {
    fn seat(&self) -> &WlSeat;
}

impl TouchDataExt for WlTouch {
    fn seat(&self) -> &WlSeat {
        self.data::<TouchData>().expect("failed to get touch data.").seat()
    }
}

sctk::delegate_touch!(WinitState);
