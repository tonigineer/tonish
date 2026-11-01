# API documentation

`just book` puts the API documentation of every crate inside this book. It includes private items, because most of the engine is private, and that is the part a developer needs:

- [renderer](../api/renderer/index.html): shapes, scenes, the CPU rasteriser, the OpenGL ES backend, PNG snapshots.
- [engine](../api/engine/index.html): the `App` trait, the Wayland side and EGL.
- [tonish](../api/tonish/index.html): the shell, its settings, messages and logging.

These links work in the built book in `book/build`, but not in `just book-serve`, which builds the book alone. To read the API documentation on its own, run `just doc --open`.
