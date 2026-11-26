# gui: drop replaced background images from the glyph cache

A config reload rebuilds the window background. A gradient layer is computed
again whenever its colors or the window's pixel size changed, and because the
gradient adds random noise to avoid banding, the new image always has a new
hash. `CachedGradient` drops the image it replaced, but
`GlyphCache::image_cache` still holds the decoded copy and keeps it until
`glyph_cache_image_cache_size` other images have pushed it out. So each such
reload kept another window-sized RGBA image: about 33 MB for a 3840x2160
window. A config that picks its background colors by the time of day, and
reloads during the day, grows the GUI by one image per gradient layer at each
change, and a window that moves between displays of different sizes does too.

When a reload replaces the background, `config_was_reloaded` now removes the
images of the layers that are no longer drawn from `image_cache`. `LfuCache`
gains `remove()`, which `put()` now uses to drop a prior value. The removed
images' atlas sprites stay until the atlas is next rebuilt, as they do for
any image the cache evicts.

Validation on the unit branch, debug builds:

A throwaway GUI with its own config: two gradient layers at 100% of the window
and `glyph_cache_image_cache_size = 1024`. The config file is rewritten three
times with unchanged colors and then ten times with new colors, and the GUI's
`MALLOC_LARGE` dirty bytes are read with macOS `footprint` after each step.

| | before | after |
|---|---|---|
| 3 reloads, unchanged colors | +0.0 MB | +0.0 MB |
| 5 reloads, new colors | +140.0 MB | +28.0 MB |
| 10 reloads, new colors | +230.8 MB | +28.0 MB |

Before, each of the first five reloads with new colors kept 28 MB, the
window's two images, and growth went on to ten. After, it stops after the
first reload. With the cache size
set to 4 instead, the growth before the change stops at 56 MB, which is how
the cache was identified as the holder.

`cargo fmt --all -- --check` passes, `cargo test -p lfucache` passes its 4
tests including a new one for `remove()`, and `cargo test -p wezterm-gui`
passes its 18 tests. There is no unit test for the reload path: `TermWindow`
cannot be built without a window and a GPU context, so it was measured as
above instead.

An AI coding agent found the growth in a long-running window, wrote the change
and ran the measurements. Nothing was tested by hand. No upstream submission
has been made.
