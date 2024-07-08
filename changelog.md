# Changelog

## 1.3

Released 2024-07-08.

 * Handle more types of drop events. This repairs dropping files from Nemo,
   which has been broken since a few weeks by a change in behavior somewhere
   in the stack.

## 1.2

Released 2023-12-30.

 * Support opening a file when provided as a command-line argument.

## 1.1

Released 2023-09-19.

 * Add an icon and `.desktop` file.
 * Accept `text/uri-list` drops, which are needed to accept drops from recent
   versions of Nautilus.
 * Update to latest version of Rust and Gtk crates.

## 1.0

Released 2019-11-04.

This is the initial release. It includes the basic application that you can
compile and run by hand, but no support files to install the application, and
no good documentation. It is useful, but not polished.

Audiograter started as and still is a hobby project, but I do use it on a
regular basis now, so I might as well put a number on it. And rather than
staying on 0.x versions eternally, I might as well call it 1.0 right now.
The version number does not imply any kind of warranty or support, it is
merely a number.
