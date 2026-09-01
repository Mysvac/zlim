# zlim-color

Color library for the zlim engine, ported from `bevy_color`.

Provides representations of colors in various color spaces, full
cross-space conversion, color operations, palettes, and GPU integration.

## Color spaces

Each color space is a distinct Rust type; every space converts to and from
every other space (`From`/`Into`):

![model_graph](docs/diagrams/model_graph.svg)

- `Srgba` / `LinearRgba` — standard (gamma-encoded) and linear sRGB.
  `LinearRgba` is what lighting calculations should use.
- `Hsla` / `Hsva` / `Hwba` — cylindrical spaces
  (hue/saturation/lightness, -value, -whiteness-blackness).
- `Laba` / `Lcha` — CIE Lab / LCH.
- `Oklaba` / `Oklcha` — perceptually uniform Oklab / Oklch.
- `Xyza` — CIE 1931 XYZ (foundational space).
- `Okhsla` / `Okhsva` / `Okhwba` — Okhsl / Okhsv / Okhwb, defined relative to
  the sRGB (Rec. 709) gamut.

## Color

- `Color` — an enum that stores any of the concrete spaces. Use it when you
  need to store a color in a data structure that cannot be generic over the
  color type.

## Traits

- `Luminance` / `Gray` / `Mix` / `Alpha` / `Hue` / `Saturation` — color
  operations implemented by every space; `Mix` handles hue wrapping along the
  short arc.
- `ColorToComponents` / `ColorToPacked` — conversions to/from `[f32; 4]`,
  `Vec3`/`Vec4`, and packed `[u8; 4]`.
- `EuclideanDistance` — distance between two colors in the same space.

## Interpolation

- `StableInterpolate` (from `zlim_math`): linear and perceptually-linear
  spaces interpolate channel-wise; cylindrical spaces interpolate through
  `Mix` so the hue wraps along the short arc; `Srgba` interpolates in linear
  space for perceptual correctness.

## Curves

- `ColorCurve<T>` — a color gradient as a `zlim_math::curve::Curve`, built on
  `EvenCore` with `Mix` interpolation.

## Palettes

- `palettes::basic` — basic colors (`RED`, `GREEN`, `BLUE`, …).
- `palettes::css` — CSS named colors.
- `palettes::tailwind` — the Tailwind palette.

## Example

```rust
use zlim_color::{Color, Hsla, Mix, Srgba};

let srgba = Srgba::new(0.5, 0.2, 0.8, 1.0);

// Every space converts to every other space:
let hsla: Hsla = srgba.into();
assert_eq!(hsla.alpha(), 1.0);

// Mix and alpha operations:
let mixed = srgba.mix(&Srgba::WHITE, 0.5);
assert_eq!(mixed.red, 0.75);

// The `Color` enum stores any space:
let color = Color::from(srgba);
assert_eq!(color.to_srgba(), srgba);
```
