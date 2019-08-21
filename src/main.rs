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
use std::fs;
use std::f32::consts;
use std::i32;

use gio::prelude::*;
use gtk::prelude::*;
use gdk::prelude::*;

/// Given t in [0, 1], return an RGB value in [0, 1]^3.
pub fn colormap_magma(t: f32) -> (f32, f32, f32) {
    // Based on https://www.shadertoy.com/view/WlfXRN (licensed CC0), which in
    // turn is a fit of https://github.com/BIDS/colormap/blob/master/colormaps.py,
    // which is also licensed CC0.

    let c: [[f32; 3]; 7] = [
        [ 18.65570506591883,    -11.48977351997711,     -5.601961508734096],
        [-50.76852536473588,     29.04658282127291,      4.23415299384598],
        [ 52.17613981234068,    -27.94360607168351,     12.94416944238394],
        [-27.66873308576866,     14.26473078096533,    -13.64921318813922],
        [  8.353717279216625,    -3.577719514958484,     0.3144679030132573],
        [  0.2516605407371642,    0.6775232436837668,    2.494026599312351],
        [ -0.002136485053939582, -0.000749655052795221, -0.005386127855323933],
    ];

    let mut result = c[0];

    // LLVM, if you can hear this comment, please unroll and vectorize.
    for j in 1..7 {
        for i in 0..3 {
            result[i] = result[i].mul_add(t, c[j][i]);
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
    flac_reader: Option<claxon::FlacReader<fs::File>>,
    target_size: (i32, i32),
    spectrum: Vec<f32>,
    sender: glib::SyncSender<ViewEvent>,
    self_sender: mpsc::SyncSender<ModelEvent>,
}

enum ModelEvent {
    OpenFile(PathBuf),
    Resize(i32, i32),
    Decode,
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
            // Stretch the bitmap to fill the entire widget. This has two
            // purposes. First, we sized the bitmap to take the DPI scaling
            // factor into account, so we may need to scale it down, because the
            // Cairo context by default measures display pixels, not device
            // pixels. Second, if you are resizing and the new pixel-perfect
            // bitmap is still being rendered, we can stretch the old one to
            // hide the fact that the new one is not ready, instead of having a
            // gap, or having the image be truncated.
            let actual_size = self.image.get_allocation();
            let scale_x = actual_size.width as f64 / pixbuf.get_width() as f64;
            let scale_y = actual_size.height as f64 / pixbuf.get_height() as f64;
            ctx.scale(scale_x, scale_y);
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
    fn new(sender: glib::SyncSender<ViewEvent>, self_sender: mpsc::SyncSender<ModelEvent>) -> Model {
        Model {
            flac_reader: None,
            spectrum: Vec::new(),
            target_size: (0, 0),
            sender: sender,
            self_sender: self_sender,
        }
    }

    pub fn run_event_loop(&mut self, events: mpsc::Receiver<ModelEvent>) {
        // Block until a new event arrives.
        for mut current_event in events.iter() {
            // When we do have an event, don't handle it immediately. First
            // check if there already is another event behind it.
            for next_event in events.try_iter() {
                current_event = match (&current_event, &next_event) {
                    // If there is another event waiting, and it is a resize
                    // just like the current event, then the current event is
                    // already obsolete and we can drop it.
                    (&ModelEvent::Resize(..), &ModelEvent::Resize(..)) => {
                        next_event
                    }
                    // In any other case, we need to handle the current event.
                    // Handle it now, and leave the next event to be handled in
                    // the next iteration, or after the `try_iter` loop.
                    _ => {
                        self.handle_event(current_event);
                        next_event
                    }
                };
            }

            self.handle_event(current_event);
        }
    }

    pub fn handle_event(&mut self, event: ModelEvent) {
        match event {
            ModelEvent::OpenFile(fname) => {
                // Build the view event in advance, so we can refuse file names
                // that we would not be able to render in the UI.
                let view_event = match fname.file_name().and_then(OsStr::to_str) {
                    // I don't care to support non-utf8 filenames.
                    None => return eprintln!("Invalid file name to open."),
                    Some(fname_str) => ViewEvent::SetTitle(fname_str.into()),
                };

                // Then try to open the file itself. If this fails, we don't
                // load the file in the UI.
                self.flac_reader = match claxon::FlacReader::open(&fname) {
                    Ok(r) => Some(r),
                    Err(err) => return eprintln!("Failed to open file: {:?}", err),
                };

                // If we have successfully loadede the file, we can tell the UI
                // to show that in the title, and we can begin decoding.
                self.sender.send(view_event).unwrap();
                self.self_sender.send(ModelEvent::Decode).unwrap();
            }
            ModelEvent::Resize(width, height) => {
                self.target_size = (width, height);
                self.repaint();
            }
            ModelEvent::Decode => {
                let flac_reader = match self.flac_reader.as_mut() {
                    Some(r) => r,
                    None => return,
                };

                let bits_per_sample = flac_reader.streaminfo().bits_per_sample;
                assert!(bits_per_sample < 32);
                let max = (i32::MAX >> (32 - bits_per_sample)) as f64;

                let mut blocks = flac_reader.blocks();
                let mut have_more = true;

                // Decode some blocks, but not everything at once. This allows
                // rendering intermediate updates, and it also keeps the app
                // more responsive by allowing us to handle other events. Doing
                // limited work and then re-posting a decode event acts like a
                // yield point.
                for i in 0..100 {
                    let block = match blocks.read_next_or_eof(Vec::new()) {
                        Ok(Some(b)) => b,
                        Ok(None) => { have_more = false; break }
                        Err(err) => return eprintln!("Failed to decode: {:?}", err),
                    };

                    let mut sum = 0.0_f64;
                    for &si in block.channel(0).iter() {
                        let sf = si as f64 / max;
                        sum = sf.mul_add(sf, sum);
                    };

                    self.spectrum.push((sum / block.duration() as f64).sqrt() as f32);
                }

                self.repaint();

                // Continue decoding.
                if have_more {
                    self.self_sender.send(ModelEvent::Decode).unwrap();
                }
            }
        }
    }

    /// Paint a new bitmap and send it over to the UI thread.
    fn repaint(&self) {
        // TODO: Paint bitmap with useful content.
        let (width, height) = self.target_size;
        assert!(width > 0);
        let bitmap = Bitmap::generate(width, height, |x, y| {
            if self.spectrum.len() == 0 { return 0.0 }
            let i = x as usize * self.spectrum.len() / width as usize;
            let ty = (y as f32 / height as f32) - 0.5;
            let sample = self.spectrum[i];
            (1.0 + sample.ln() * 0.1).max(0.0) * (ty * consts::PI).cos()
        });
        self.sender.send(ViewEvent::SetView(bitmap)).unwrap();
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
        let send_model_clone = send_model.clone();
        thread::spawn(move || {
            let mut model = Model::new(send_view, send_model_clone);
            model.run_event_loop(recv_model);
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
