#!/usr/bin/env python3
"""Hote GTK4 + WebKitGTK — banc de mesure du HUD."""
import sys

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("WebKit", "6.0")
from gi.repository import Gdk, Gtk, WebKit  # noqa: E402

URI = sys.argv[1]
FULLSCREEN = "--fullscreen" in sys.argv


class Host(Gtk.Application):
    def __init__(self):
        super().__init__(application_id="dev.hudbench.webkit")

    def do_activate(self):
        win = Gtk.ApplicationWindow(application=self)
        win.set_title("hudbench")
        win.set_default_size(320, 140)
        view = WebKit.WebView()
        transparent = Gdk.RGBA()
        transparent.parse("rgba(0,0,0,0)")
        view.set_background_color(transparent)
        # Without this, console.log() inside the page never reaches our stdout,
        # so the probe's VISPROBE_* markers would be invisible to the harness.
        view.get_settings().set_enable_write_console_messages_to_stdout(True)
        view.load_uri(URI)
        win.set_child(view)
        if FULLSCREEN:
            win.fullscreen()
        win.present()


Host().run([sys.argv[0]])
