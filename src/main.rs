// Spekje -- Spectrogram viewer
// Copyright 2019 Ruud van Asseldonk

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 3. A copy
// of the License is available in the root of the repository.

use std::env;

use gio::prelude::*;
use gtk::prelude::*;

fn build_canvas() -> Option<gdk_pixbuf::Pixbuf> {
    let has_alpha = false;
    let bits_per_sample = 8;
    let width = 1280;
    let height = 720;
    gdk_pixbuf::Pixbuf::new(
        gdk_pixbuf::Colorspace::Rgb,
        has_alpha,
        bits_per_sample,
        width,
        height,
    )
}

fn build_ui(application: &gtk::Application) {
    let window = gtk::ApplicationWindow::new(application);

    window.set_title("Spekje");
    window.set_border_width(10);
    window.set_position(gtk::WindowPosition::Center);
    window.set_default_size(1280, 720);

    let vbox = gtk::Box::new(
        gtk::Orientation::Vertical,
        10,
    );
    window.add(&vbox);

    let canvas = build_canvas();
    let image = gtk::Image::new_from_pixbuf(canvas.as_ref());

    let expand = true;
    let fill = true;
    let padding = 0;
    vbox.pack_start(&image, expand, fill, padding);

    // Accept single strings for dropping. We could accept "text/uri-list" too,
    // but the application cannot handle more than one file at a time anyway.
    let drag_event_info = 0;
    let drag_targets = [
        gtk::TargetEntry::new(
            "text/plain",
            gtk::TargetFlags::OTHER_APP,
            drag_event_info,
        ),
    ];

    window.drag_dest_set(
        gtk::DestDefaults::ALL,
        &drag_targets[..],
        gdk::DragAction::COPY,
    );

    window.connect_drag_data_received(move |_self, _drag_context, _x, _y, data, info, _time| {
        // We registered only this target, so we should only be signalled for
        // this target.
        assert_eq!(info, drag_event_info);
        if let Some(uri) = data.get_text() {
            if let Ok(fname) = glib::filename_from_uri(uri.as_str()) {
                println!("{:?}", fname);
            }
        }
    });

    window.show_all();
}

fn main() {
    let application = gtk::Application::new(
        Some("nl.ruuda.spekje"),
        Default::default(),
    ).unwrap();

    application.connect_activate(move |app| {
        build_ui(app);
    });

    let args: Vec<_> = env::args().collect();
    application.run(&args);
}
