// Spekje -- Spectrogram viewer
// Copyright 2019 Ruud van Asseldonk

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 3. A copy
// of the License is available in the root of the repository.

use std::env;
use std::iter;
use std::path::{PathBuf};
use std::ffi::OsStr;
use std::sync::mpsc;
use std::thread;
use std::cell::RefCell;
use std::rc::Rc;

use gio::prelude::*;
use gtk::prelude::*;
use gdk::prelude::*;

/// Given t in [0, 1], return an RGB value in [0, 1]^3.
pub fn colormap_magma(t: f32) -> (f32, f32, f32) {
    // Based on https://www.shadertoy.com/view/WlfXRN (licensed CC0), which in
    // turn is a fit of https://github.com/BIDS/colormap/blob/master/colormaps.py,
    // which is also licensed CC0.

    let c = [
        [ -0.002136485053939582, -0.000749655052795221, -0.005386127855323933],
        [  0.2516605407371642,    0.6775232436837668,    2.494026599312351],
        [  8.353717279216625,    -3.577719514958484,     0.3144679030132573],
        [-27.66873308576866,     14.26473078096533,    -13.64921318813922],
        [ 52.17613981234068,    -27.94360607168351,     12.94416944238394],
        [-50.76852536473588,     29.04658282127291,      4.23415299384598],
        [ 18.65570506591883,    -11.48977351997711,     -5.601961508734096],
    ];

    let mut result = [0.0, 0.0, 0.0];

    for j in (0..7).rev() {
        for i in 0..3 {
            result[i] *= t;
            result[i] += c[j][i];
        }
    }

    (result[0], result[1], result[2])
}


/// Thread-safe bitmap that we can fill on one thread and display on another.
struct Bitmap {
    data: Vec<u8>,
    width: i32,
    height: i32,
}

impl Bitmap {
    pub fn new(width: i32, height: i32) -> Bitmap {
        let len = width * height * 3;
        Bitmap {
            data: iter::repeat(0).take(len as usize).collect(),
            width: width,
            height: height,
        }
    }

    pub fn generate<F: Fn(i32, i32) -> f32>(width: i32, height: i32, f: F) -> Bitmap {
        let len = width * height * 3;
        let mut data = Vec::with_capacity(len as usize);

        for y in 0..height {
            for x in 0..width {
                let t = f(x, y);
                let (r, g, b) = colormap_magma(t);
                data.push((r.min(1.0).max(0.0) * 255.0) as u8);
                data.push((g.min(1.0).max(0.0) * 255.0) as u8);
                data.push((b.min(1.0).max(0.0) * 255.0) as u8);
            }
        }

        Bitmap {
            data,
            width,
            height,
        }
    }

    pub fn into_pixbuf(self) -> gdk_pixbuf::Pixbuf {
        let has_alpha = false;
        let bits_per_sample = 8;
        let row_stride = 3 * self.width;
        gdk_pixbuf::Pixbuf::new_from_mut_slice(
            self.data,
            gdk_pixbuf::Colorspace::Rgb,
            has_alpha,
            bits_per_sample,
            self.width,
            self.height,
            row_stride,
        )
    }
}

/// Container for the application widgets.
///
/// Although GTK widgets are already refcounted, the view itself is also kept in
/// a refcounted cell. This allows events to mutate the view state, e.g. in
/// order to swap out the pixbuf.
struct View {
    window: gtk::ApplicationWindow,
    image: gtk::DrawingArea,
    pixbuf: Option<gdk_pixbuf::Pixbuf>,
    sender: mpsc::SyncSender<ModelEvent>,
}

enum ViewEvent {
    SetTitle(String),
    SetView(Bitmap),
}

struct Model {
    file_name: Option<PathBuf>,
    target_size: (i32, i32),
    sender: glib::SyncSender<ViewEvent>,
}

enum ModelEvent {
    OpenFile(PathBuf),
    Resize(i32, i32),
}

impl View {
    fn new(
        application: &gtk::Application,
        sender: mpsc::SyncSender<ModelEvent>,
    ) -> Rc<RefCell<View>> {
        let window = gtk::ApplicationWindow::new(application);

        window.set_title("Spekje");
        window.set_border_width(10);
        window.set_position(gtk::WindowPosition::Center);
        window.set_default_size(640, 480);

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 10);
        window.add(&vbox);

        let image = gtk::DrawingArea::new();

        let expand = true;
        let fill = true;
        let padding = 0;
        vbox.pack_start(&image, expand, fill, padding);

        // Accept single strings for dropping. We could accept "text/uri-list" too,
        // but the application cannot handle more than one file at a time anyway.
        const DRAG_EVENT_INFO: u32 = 0;
        let drag_targets = [
            gtk::TargetEntry::new(
                "text/plain",
                gtk::TargetFlags::OTHER_APP,
                DRAG_EVENT_INFO,
            ),
        ];

        window.drag_dest_set(
            gtk::DestDefaults::ALL,
            &drag_targets[..],
            gdk::DragAction::COPY,
        );


        let view_cell = Rc::new(RefCell::new(
            View {
                window: window.clone(),
                image: image.clone(),
                pixbuf: None,
                sender: sender,
            }
        ));

        let view_cell_clone = view_cell.clone();
        window.connect_drag_data_received(move |_self, _drag_context, _x, _y, data, info, _time| {
            assert_eq!(info, DRAG_EVENT_INFO);
            view_cell_clone.borrow_mut().on_drag_data_received(data);
        });

        let view_cell_clone = view_cell.clone();
        image.connect_draw(move |_self, ctx| {
            view_cell_clone.borrow_mut().on_draw(ctx);
            glib::signal::Inhibit(true)
        });

        let view_cell_clone = view_cell.clone();
        image.connect_size_allocate(move |_self, rect| {
            view_cell_clone.borrow_mut().on_size_allocate(rect);
        });

        window.show_all();

        view_cell
    }

    fn on_drag_data_received(&self, data: &gtk::SelectionData) {
        // We registered only this target, so we should only be signalled for
        // this target.
        if let Some(uri) = data.get_text() {
            // When dropped, the uri is terminated by a newline. Strip it.
            let uri_stripped = uri.as_str().trim_end();
            if let Ok((fname, _)) = glib::filename_from_uri(uri_stripped) {
                self.sender.send(ModelEvent::OpenFile(fname)).unwrap();
            }
        }
    }

    fn on_size_allocate(&self, rect: &gtk::Rectangle) {
        let f = self.image.get_scale_factor();
        self.sender.send(ModelEvent::Resize(rect.width * f, rect.height * f)).unwrap();
    }

    fn on_draw(&self, ctx: &cairo::Context) {
        if let Some(pixbuf) = self.pixbuf.as_ref() {
            // The pixmap is blown up by the scaling factor, so it fills the
            // widgets size in device pixels. But the Cairo context's default
            // scale is device-independent, which is right for DPI-unaware apps.
            // We are aware, so scale down to make the pixmap fill the widget.
            let f = (self.image.get_scale_factor() as f64).recip();
            ctx.scale(f, f);
            ctx.set_source_pixbuf(pixbuf, 0.0, 0.0);
            ctx.paint();
        }
    }

    /// Handle one event. Should only be called on the main thread.
    fn handle_event(&mut self, event: ViewEvent) {
        match event {
            ViewEvent::SetTitle(fname) => {
                self.window.set_title(&format!("{} - Spekje", fname));
            }
            ViewEvent::SetView(bitmap) => {
                self.pixbuf = Some(bitmap.into_pixbuf());
                self.image.queue_draw();
            }
        }
    }
}

impl Model {
    fn new(sender: glib::SyncSender<ViewEvent>) -> Model {
        Model {
            file_name: None,
            target_size: (0, 0),
            sender: sender,
        }
    }

    pub fn handle_event(&mut self, event: ModelEvent) {
        match event {
            ModelEvent::OpenFile(fname) => {
                match fname.file_name().and_then(OsStr::to_str) {
                    // I don't care to support non-utf8 filenames.
                    None => return eprintln!("Invalid file name to open."),
                    Some(fname_str) => {
                        let event = ViewEvent::SetTitle(fname_str.into());
                        self.sender.send(event).unwrap();
                    },
                }
                self.file_name = Some(fname);
                // TODO: Start actual file load.
            }
            ModelEvent::Resize(width, height) => {
                self.target_size = (width, height);
                // TODO: Paint bitmap with useful content.
                let bitmap = Bitmap::generate(width, height, |x, y| {
                    let ty = y as f32 / height as f32;
                    let tx = x as f32 / width as f32;
                    (7.0 * tx).sin() * (tx.sin() * ty).cos() * 0.5 + 0.5
                });
                self.sender.send(ViewEvent::SetView(bitmap)).unwrap();
            }
        }
    }
}

fn main() {
    let application = gtk::Application::new(
        Some("nl.ruuda.spekje"),
        Default::default(),
    ).unwrap();

    // When the application starts, run all of this on the main thread.
    application.connect_activate(move |app| {
        // Create two bounded one-way message queues. The one that sends
        // messages back to the view is a tailored glib channel, but it behaves
        // the same as the mpsc one.
        let (send_model, recv_model) = mpsc::sync_channel(10);
        let (send_view, recv_view) = glib::MainContext::sync_channel(glib::PRIORITY_DEFAULT_IDLE, 10);

        // On a background thread, construct the model, and run its event loop.
        thread::spawn(move || {
            let mut model = Model::new(send_view);
            loop {
                match recv_model.recv() {
                    Ok(event) => model.handle_event(event),
                    Err(_) => break,
                };
            }
        });

        // Back on the main thread, construct the view.
        let view_cell = View::new(app, send_model);

        // Handle the view's events on this thread, the main thread.
        recv_view.attach(None, move |event| {
            view_cell.borrow_mut().handle_event(event);
            glib::source::Continue(true)
        });
    });

    // And run the UI event loop on the main thread.
    let args: Vec<_> = env::args().collect();
    application.run(&args);
}
