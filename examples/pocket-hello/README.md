# pocket-hello

First power-up test for the ull-pocket board (ESP32-C6). The user LED shows
the held button (UP red, OK blue, DN both); the log prints the buttons, USB
power and the wake lines once a second. No Wi-Fi, no sensors.

```sh
cargo c6-build --release -p pocket-hello
cargo c6-run --release -p pocket-hello   # flash over USB-C and show the log
```

Not run on hardware yet: the board has not been built.
