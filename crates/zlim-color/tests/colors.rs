//! Round-trip conversion tests across all color spaces.
//!
//! The [`TEST_COLORS`] table holds equivalent colors in every space; each
//! test converts a color out of its source space into another and back,
//! checking the result against the table.

use zlim_color::color_difference::EuclideanDistance;
use zlim_color::{Hsla, Hsva, Hwba, Laba, Lcha, LinearRgba};
use zlim_color::{Mix, Okhsla, Okhsva, Okhwba, Oklaba, Oklcha, Srgba, Xyza};
use zlim_math::ops;

macro_rules! assert_approx_eq {
    ($x:expr, $y:expr, $d:expr) => {
        assert!(!f32::is_nan($x));
        assert!(!f32::is_nan($y));
        if ops::abs($x - $y) >= $d {
            panic!(
                "assertion failed: `(left !== right)` \
                 (left: `{}`, right: `{}`, tolerance: `{}`)",
                $x, $y, $d
            );
        }
    };

    ($x:expr, $y:expr, $d:expr, $msg:expr) => {
        assert!(!f32::is_nan($x));
        assert!(!f32::is_nan($y));
        if ops::abs($x - $y) >= $d {
            panic!(
                "assertion failed: `(left !== right)` \
                 (left: `{}`, right: `{}`, tolerance: `{}`). {}",
                $x, $y, $d, $msg
            );
        }
    };
}

struct TestColor {
    name: &'static str,
    rgb: Srgba,
    linear_rgb: LinearRgba,
    hsl: Hsla,
    hsv: Hsva,
    hwb: Hwba,
    lab: Laba,
    lch: Lcha,
    oklab: Oklaba,
    oklch: Oklcha,
    xyz: Xyza,
    okhsl: Okhsla,
    okhsv: Okhsva,
    okhwb: Okhwba,
}

// Table of equivalent colors in various color spaces
const TEST_COLORS: &[TestColor] = &[
    // black
    TestColor {
        name: "black",
        rgb: Srgba::new(0.0, 0.0, 0.0, 1.0),
        linear_rgb: LinearRgba::new(0.0, 0.0, 0.0, 1.0),
        hsl: Hsla::new(0.0, 0.0, 0.0, 1.0),
        hsv: Hsva::new(0.0, 0.0, 0.0, 1.0),
        hwb: Hwba::new(0.0, 0.0, 1.0, 1.0),
        lab: Laba::new(0.0, 0.0, 0.0, 1.0),
        lch: Lcha::new(0.0, 0.0, 0.0, 1.0),
        oklab: Oklaba::new(0.0, 0.0, 0.0, 1.0),
        oklch: Oklcha::new(0.0, 0.0, 0.0, 1.0),
        xyz: Xyza::new(0.0, 0.0, 0.0, 1.0),
        okhsl: Okhsla::new(0.0, 0.0, 0.0, 1.0),
        okhsv: Okhsva::new(0.0, 0.0, 0.0, 1.0),
        okhwb: Okhwba::new(0.0, 0.0, 1.0, 1.0),
    },
    // black_plus_epsilon
    TestColor {
        name: "black_plus_epsilon",
        rgb: Srgba::new(0.0, 0.0, 0.0000002, 1.0),
        linear_rgb: LinearRgba::new(0.0, 0.0, 0.000000015479877, 1.0),
        hsl: Hsla::new(240.0, 1.0, 0.0000001, 1.0),
        hsv: Hsva::new(240.0, 1.0, 0.0000002, 1.0),
        hwb: Hwba::new(240.0, 0.0, 0.9999998, 1.0),
        lab: Laba::new(0.000000019073486, 0.000000074505806, -0.00000017881393, 1.0),
        lch: Lcha::new(0.000000019073486, 0.0000001937151, 292.61987, 1.0),
        oklab: Oklaba::new(0.0011265249, -0.00008089037, -0.00077640166, 1.0),
        oklch: Oklcha::new(0.0011265249, 0.00078060414, 264.05203, 1.0),
        xyz: Xyza::new(
            0.0000000027931504,
            0.0000000011172602,
            0.00000001471059,
            1.0,
        ),
        okhsl: Okhsla::new(264.05203, 0.9999977, 0.00019314568, 1.0),
        okhsv: Okhsva::new(264.05203, 0.9999911, 0.00049864844, 1.0),
        okhwb: Okhwba::new(264.05203, 0.000000004428543, 0.99950135, 1.0),
    },
    // white_minus_epsilon
    TestColor {
        name: "white_minus_epsilon",
        rgb: Srgba::new(1.0, 1.0, 0.9999998, 1.0),
        linear_rgb: LinearRgba::new(1.0, 1.0, 0.9999996, 1.0),
        hsl: Hsla::new(60.0, 0.75, 0.9999999, 1.0),
        hsv: Hsva::new(60.0, 0.00000017881393, 1.0, 1.0),
        hwb: Hwba::new(60.0, 0.9999998, 0.0, 1.0),
        lab: Laba::new(1.0, 0.0, 0.00000023841858, 1.0),
        lch: Lcha::new(1.0, 0.00000023841858, 90.0, 1.0),
        oklab: Oklaba::new(1.0, -0.000000029802322, 0.00000011920929, 1.0),
        oklch: Oklcha::new(1.0, 0.00000012287812, 104.03625, 1.0),
        xyz: Xyza::new(0.95047, 1.0, 1.0888295, 1.0),
        okhsl: Okhsla::new(0.0, 0.0, 1.0, 1.0),
        okhsv: Okhsva::new(0.0, 0.0, 1.0, 1.0),
        okhwb: Okhwba::new(0.0, 1.0, 0.0, 1.0),
    },
    // white
    TestColor {
        name: "white",
        rgb: Srgba::new(1.0, 1.0, 1.0, 1.0),
        linear_rgb: LinearRgba::new(1.0, 1.0, 1.0, 1.0),
        hsl: Hsla::new(0.0, 0.0, 1.0, 1.0),
        hsv: Hsva::new(0.0, 0.0, 1.0, 1.0),
        hwb: Hwba::new(0.0, 1.0, 0.0, 1.0),
        lab: Laba::new(1.0, 0.0, 0.0, 1.0),
        lch: Lcha::new(1.0, 0.0, 0.0, 1.0),
        oklab: Oklaba::new(1.0, 0.0, 0.000000059604645, 1.0),
        oklch: Oklcha::new(1.0, 0.000000059604645, 90.0, 1.0),
        xyz: Xyza::new(0.95047, 1.0, 1.08883, 1.0),
        okhsl: Okhsla::new(0.0, 0.0, 1.0, 1.0),
        okhsv: Okhsva::new(0.0, 0.0, 1.0, 1.0),
        okhwb: Okhwba::new(0.0, 1.0, 0.0, 1.0),
    },
    // red
    TestColor {
        name: "red",
        rgb: Srgba::new(1.0, 0.0, 0.0, 1.0),
        linear_rgb: LinearRgba::new(1.0, 0.0, 0.0, 1.0),
        hsl: Hsla::new(0.0, 1.0, 0.5, 1.0),
        hsv: Hsva::new(0.0, 1.0, 1.0, 1.0),
        hwb: Hwba::new(0.0, 0.0, 0.0, 1.0),
        lab: Laba::new(0.53240794, 0.8009246, 0.67203194, 1.0),
        lch: Lcha::new(0.53240794, 1.0455177, 39.99901, 1.0),
        oklab: Oklaba::new(0.6279554, 0.22486295, 0.1258463, 1.0),
        oklch: Oklcha::new(0.6279554, 0.25768322, 29.233906, 1.0),
        xyz: Xyza::new(0.4124564, 0.2126729, 0.0193339, 1.0),
        okhsl: Okhsla::new(29.233885, 1.0, 0.56808466, 1.0),
        okhsv: Okhsva::new(29.233885, 1.0, 1.0, 1.0),
        okhwb: Okhwba::new(29.233885, 0.0, 0.0, 1.0),
    },
    // green
    TestColor {
        name: "green",
        rgb: Srgba::new(0.0, 1.0, 0.0, 1.0),
        linear_rgb: LinearRgba::new(0.0, 1.0, 0.0, 1.0),
        hsl: Hsla::new(120.0, 1.0, 0.5, 1.0),
        hsv: Hsva::new(120.0, 1.0, 1.0, 1.0),
        hwb: Hwba::new(120.0, 0.0, 0.0, 1.0),
        lab: Laba::new(0.87734723, -0.86182714, 0.8317932, 1.0),
        lch: Lcha::new(0.87734723, 1.1977587, 136.01595, 1.0),
        oklab: Oklaba::new(0.8664396, -0.2338874, 0.1794985, 1.0),
        oklch: Oklcha::new(0.8664396, 0.2948271, 142.49532, 1.0),
        xyz: Xyza::new(0.3575761, 0.7151522, 0.119192, 1.0),
        okhsl: Okhsla::new(142.49535, 0.99999994, 0.844529, 1.0),
        okhsv: Okhsva::new(142.49535, 0.9999999, 1.0, 1.0),
        okhwb: Okhwba::new(142.49535, 0.00000011920929, 0.0, 1.0),
    },
    // blue
    TestColor {
        name: "blue",
        rgb: Srgba::new(0.0, 0.0, 1.0, 1.0),
        linear_rgb: LinearRgba::new(0.0, 0.0, 1.0, 1.0),
        hsl: Hsla::new(240.0, 1.0, 0.5, 1.0),
        hsv: Hsva::new(240.0, 1.0, 1.0, 1.0),
        hwb: Hwba::new(240.0, 0.0, 0.0, 1.0),
        lab: Laba::new(0.32297012, 0.7918753, -1.0786016, 1.0),
        lch: Lcha::new(0.32297012, 1.3380761, 306.28494, 1.0),
        oklab: Oklaba::new(0.4520137, -0.032456964, -0.31152815, 1.0),
        oklch: Oklcha::new(0.4520137, 0.31321436, 264.05203, 1.0),
        xyz: Xyza::new(0.1804375, 0.072175, 0.9503041, 1.0),
        okhsl: Okhsla::new(264.05203, 1.0, 0.36656535, 1.0),
        okhsv: Okhsva::new(264.05203, 1.0, 0.99999994, 1.0),
        okhwb: Okhwba::new(264.05203, 0.0, 0.000000059604645, 1.0),
    },
    // yellow
    TestColor {
        name: "yellow",
        rgb: Srgba::new(1.0, 1.0, 0.0, 1.0),
        linear_rgb: LinearRgba::new(1.0, 1.0, 0.0, 1.0),
        hsl: Hsla::new(60.0, 1.0, 0.5, 1.0),
        hsv: Hsva::new(60.0, 1.0, 1.0, 1.0),
        hwb: Hwba::new(60.0, 0.0, 0.0, 1.0),
        lab: Laba::new(0.9713927, -0.21553755, 0.94477975, 1.0),
        lch: Lcha::new(0.9713927, 0.96905375, 102.85126, 1.0),
        oklab: Oklaba::new(0.9679827, -0.07136908, 0.19856972, 1.0),
        oklch: Oklcha::new(0.9679827, 0.21100587, 109.76924, 1.0),
        xyz: Xyza::new(0.7700325, 0.9278251, 0.1385259, 1.0),
        okhsl: Okhsla::new(109.76923, 1.0, 0.9627044, 1.0),
        okhsv: Okhsva::new(109.76923, 1.0000005, 1.0, 1.0),
        okhwb: Okhwba::new(109.76923, -0.00000047683716, 0.0, 1.0),
    },
    // magenta
    TestColor {
        name: "magenta",
        rgb: Srgba::new(1.0, 0.0, 1.0, 1.0),
        linear_rgb: LinearRgba::new(1.0, 0.0, 1.0, 1.0),
        hsl: Hsla::new(300.0, 1.0, 0.5, 1.0),
        hsv: Hsva::new(300.0, 1.0, 1.0, 1.0),
        hwb: Hwba::new(300.0, 0.0, 0.0, 1.0),
        lab: Laba::new(0.6032421, 0.9823433, -0.60824895, 1.0),
        lch: Lcha::new(0.6032421, 1.1554068, 328.23495, 1.0),
        oklab: Oklaba::new(0.7016738, 0.27456632, -0.16915613, 1.0),
        oklch: Oklcha::new(0.7016738, 0.32249102, 328.36343, 1.0),
        xyz: Xyza::new(0.5928939, 0.28484792, 0.969638, 1.0),
        okhsl: Okhsla::new(328.3634, 1.0, 0.65329874, 1.0),
        okhsv: Okhsva::new(328.3634, 1.0, 1.0, 1.0),
        okhwb: Okhwba::new(328.3634, 0.0, 0.0, 1.0),
    },
    // cyan
    TestColor {
        name: "cyan",
        rgb: Srgba::new(0.0, 1.0, 1.0, 1.0),
        linear_rgb: LinearRgba::new(0.0, 1.0, 1.0, 1.0),
        hsl: Hsla::new(180.0, 1.0, 0.5, 1.0),
        hsv: Hsva::new(180.0, 1.0, 1.0, 1.0),
        hwb: Hwba::new(180.0, 0.0, 0.0, 1.0),
        lab: Laba::new(0.9111322, -0.48087537, -0.14131176, 1.0),
        lch: Lcha::new(0.9111322, 0.50120866, 196.37614, 1.0),
        oklab: Oklaba::new(0.90539926, -0.1494439, -0.039398134, 1.0),
        oklch: Oklcha::new(0.90539926, 0.15454996, 194.76895, 1.0),
        xyz: Xyza::new(0.5380136, 0.78732723, 1.069496, 1.0),
        okhsl: Okhsla::new(194.76895, 1.0, 0.8898483, 1.0),
        okhsv: Okhsva::new(194.76895, 0.9999998, 1.0, 1.0),
        okhwb: Okhwba::new(194.76895, 0.00000017881393, 0.0, 1.0),
    },
    // gray
    TestColor {
        name: "gray",
        rgb: Srgba::new(0.5, 0.5, 0.5, 1.0),
        linear_rgb: LinearRgba::new(0.21404114, 0.21404114, 0.21404114, 1.0),
        hsl: Hsla::new(0.0, 0.0, 0.5, 1.0),
        hsv: Hsva::new(0.0, 0.0, 0.5, 1.0),
        hwb: Hwba::new(0.0, 0.5, 0.5, 1.0),
        lab: Laba::new(0.5338897, 0.0, 0.00000011920929, 1.0),
        lch: Lcha::new(0.5338897, 0.00000011920929, 90.0, 1.0),
        oklab: Oklaba::new(0.5981807, 0.00000011920929, 0.0, 1.0),
        oklch: Oklcha::new(0.5981807, 0.00000011920929, 0.0, 1.0),
        xyz: Xyza::new(0.2034397, 0.21404117, 0.23305441, 1.0),
        okhsl: Okhsla::new(0.0, 0.0, 0.53375983, 1.0),
        okhsv: Okhsva::new(0.0, 0.0, 0.53375983, 1.0),
        okhwb: Okhwba::new(0.0, 0.53375983, 0.46624017, 1.0),
    },
    // olive
    TestColor {
        name: "olive",
        rgb: Srgba::new(0.5, 0.5, 0.0, 1.0),
        linear_rgb: LinearRgba::new(0.21404114, 0.21404114, 0.0, 1.0),
        hsl: Hsla::new(60.0, 1.0, 0.25, 1.0),
        hsv: Hsva::new(60.0, 1.0, 0.5, 1.0),
        hwb: Hwba::new(60.0, 0.0, 0.5, 1.0),
        lab: Laba::new(0.51677734, -0.12893051, 0.5651491, 1.0),
        lch: Lcha::new(0.51677734, 0.57966936, 102.851265, 1.0),
        oklab: Oklaba::new(0.57902855, -0.042691574, 0.11878061, 1.0),
        oklch: Oklcha::new(0.57902855, 0.12621966, 109.76922, 1.0),
        xyz: Xyza::new(0.16481864, 0.19859275, 0.029650241, 1.0),
        okhsl: Okhsla::new(109.76923, 1.0000005, 0.51171625, 1.0),
        okhsv: Okhsva::new(109.76923, 1.0000005, 0.5318635, 1.0),
        okhwb: Okhwba::new(109.76923, -0.00000025361228, 0.4681365, 1.0),
    },
    // purple
    TestColor {
        name: "purple",
        rgb: Srgba::new(0.5, 0.0, 0.5, 1.0),
        linear_rgb: LinearRgba::new(0.21404114, 0.0, 0.21404114, 1.0),
        hsl: Hsla::new(300.0, 1.0, 0.25, 1.0),
        hsv: Hsva::new(300.0, 1.0, 0.5, 1.0),
        hwb: Hwba::new(300.0, 0.0, 0.5, 1.0),
        lab: Laba::new(0.29655674, 0.58761877, -0.3638428, 1.0),
        lch: Lcha::new(0.29655674, 0.69114214, 328.23495, 1.0),
        oklab: Oklaba::new(0.41972777, 0.1642403, -0.10118592, 1.0),
        oklch: Oklcha::new(0.41972777, 0.19290791, 328.36343, 1.0),
        xyz: Xyza::new(0.12690368, 0.060969174, 0.20754242, 1.0),
        okhsl: Okhsla::new(328.3634, 0.99999994, 0.33011043, 1.0),
        okhsv: Okhsva::new(328.3634, 1.0, 0.5106205, 1.0),
        okhwb: Okhwba::new(328.3634, 0.0, 0.48937953, 1.0),
    },
    // teal
    TestColor {
        name: "teal",
        rgb: Srgba::new(0.0, 0.5, 0.5, 1.0),
        linear_rgb: LinearRgba::new(0.0, 0.21404114, 0.21404114, 1.0),
        hsl: Hsla::new(180.0, 1.0, 0.25, 1.0),
        hsv: Hsva::new(180.0, 1.0, 0.5, 1.0),
        hwb: Hwba::new(180.0, 0.0, 0.5, 1.0),
        lab: Laba::new(0.48073065, -0.28765038, -0.08452999, 1.0),
        lch: Lcha::new(0.48073065, 0.29981336, 196.37614, 1.0),
        oklab: Oklaba::new(0.54159236, -0.08939436, -0.02356726, 1.0),
        oklch: Oklcha::new(0.54159236, 0.09244873, 194.769, 1.0),
        xyz: Xyza::new(0.11515705, 0.16852042, 0.22891617, 1.0),
        okhsl: Okhsla::new(194.76895, 0.9999998, 0.46872336, 1.0),
        okhsv: Okhsva::new(194.76895, 0.9999998, 0.52782416, 1.0),
        okhwb: Okhwba::new(194.76895, 0.000000094382315, 0.47217584, 1.0),
    },
    // maroon
    TestColor {
        name: "maroon",
        rgb: Srgba::new(0.5, 0.0, 0.0, 1.0),
        linear_rgb: LinearRgba::new(0.21404114, 0.0, 0.0, 1.0),
        hsl: Hsla::new(0.0, 1.0, 0.25, 1.0),
        hsv: Hsva::new(0.0, 1.0, 0.5, 1.0),
        hwb: Hwba::new(0.0, 0.0, 0.5, 1.0),
        lab: Laba::new(0.2541851, 0.47909766, 0.37905872, 1.0),
        lch: Lcha::new(0.2541851, 0.61091745, 38.350803, 1.0),
        oklab: Oklaba::new(0.3756308, 0.13450874, 0.07527886, 1.0),
        oklch: Oklcha::new(0.3756308, 0.1541412, 29.233906, 1.0),
        xyz: Xyza::new(0.08828264, 0.045520753, 0.0041382504, 1.0),
        okhsl: Okhsla::new(29.233885, 1.0, 0.28080443, 1.0),
        okhsv: Okhsva::new(29.233885, 1.0, 0.50226027, 1.0),
        okhwb: Okhwba::new(29.233885, 0.0, 0.49773973, 1.0),
    },
    // lime
    TestColor {
        name: "lime",
        rgb: Srgba::new(0.0, 0.5, 0.0, 1.0),
        linear_rgb: LinearRgba::new(0.0, 0.21404114, 0.0, 1.0),
        hsl: Hsla::new(120.0, 1.0, 0.25, 1.0),
        hsv: Hsva::new(120.0, 1.0, 0.5, 1.0),
        hwb: Hwba::new(120.0, 0.0, 0.5, 1.0),
        lab: Laba::new(0.46052113, -0.5155285, 0.4975627, 1.0),
        lch: Lcha::new(0.46052113, 0.71647626, 136.01596, 1.0),
        oklab: Oklaba::new(0.5182875, -0.13990697, 0.10737252, 1.0),
        oklch: Oklcha::new(0.5182875, 0.17635992, 142.49535, 1.0),
        xyz: Xyza::new(0.076536, 0.153072, 0.025511991, 1.0),
        okhsl: Okhsla::new(142.49535, 1.0, 0.44203484, 1.0),
        okhsv: Okhsva::new(142.49535, 0.9999999, 0.5250593, 1.0),
        okhwb: Okhwba::new(142.49535, 0.000000062591944, 0.47494072, 1.0),
    },
    // navy
    TestColor {
        name: "navy",
        rgb: Srgba::new(0.0, 0.0, 0.5, 1.0),
        linear_rgb: LinearRgba::new(0.0, 0.0, 0.21404114, 1.0),
        hsl: Hsla::new(240.0, 1.0, 0.25, 1.0),
        hsv: Hsva::new(240.0, 1.0, 0.5, 1.0),
        hwb: Hwba::new(240.0, 0.0, 0.5, 1.0),
        lab: Laba::new(0.12890343, 0.4736845, -0.64519864, 1.0),
        lch: Lcha::new(0.12890343, 0.8004114, 306.28494, 1.0),
        oklab: Oklaba::new(0.27038592, -0.01941514, -0.18635012, 1.0),
        oklch: Oklcha::new(0.27038592, 0.18735878, 264.05203, 1.0),
        xyz: Xyza::new(0.03862105, 0.01544842, 0.20340417, 1.0),
        okhsl: Okhsla::new(264.05203, 1.0, 0.16734318, 1.0),
        okhsv: Okhsva::new(264.05203, 1.0, 0.47496656, 1.0),
        okhwb: Okhwba::new(264.05203, 0.0, 0.5250335, 1.0),
    },
    // orange
    TestColor {
        name: "orange",
        rgb: Srgba::new(0.5, 0.5, 0.0, 1.0),
        linear_rgb: LinearRgba::new(0.21404114, 0.21404114, 0.0, 1.0),
        hsl: Hsla::new(60.0, 1.0, 0.25, 1.0),
        hsv: Hsva::new(60.0, 1.0, 0.5, 1.0),
        hwb: Hwba::new(60.0, 0.0, 0.5, 1.0),
        lab: Laba::new(0.51677734, -0.12893051, 0.5651491, 1.0),
        lch: Lcha::new(0.51677734, 0.57966936, 102.851265, 1.0),
        oklab: Oklaba::new(0.57902855, -0.042691574, 0.11878061, 1.0),
        oklch: Oklcha::new(0.57902855, 0.12621966, 109.76922, 1.0),
        xyz: Xyza::new(0.16481864, 0.19859275, 0.029650241, 1.0),
        okhsl: Okhsla::new(109.76923, 1.0000005, 0.51171625, 1.0),
        okhsv: Okhsva::new(109.76923, 1.0000005, 0.5318635, 1.0),
        okhwb: Okhwba::new(109.76923, -0.00000025361228, 0.4681365, 1.0),
    },
    // fuchsia
    TestColor {
        name: "fuchsia",
        rgb: Srgba::new(0.5, 0.0, 0.5, 1.0),
        linear_rgb: LinearRgba::new(0.21404114, 0.0, 0.21404114, 1.0),
        hsl: Hsla::new(300.0, 1.0, 0.25, 1.0),
        hsv: Hsva::new(300.0, 1.0, 0.5, 1.0),
        hwb: Hwba::new(300.0, 0.0, 0.5, 1.0),
        lab: Laba::new(0.29655674, 0.58761877, -0.3638428, 1.0),
        lch: Lcha::new(0.29655674, 0.69114214, 328.23495, 1.0),
        oklab: Oklaba::new(0.41972777, 0.1642403, -0.10118592, 1.0),
        oklch: Oklcha::new(0.41972777, 0.19290791, 328.36343, 1.0),
        xyz: Xyza::new(0.12690368, 0.060969174, 0.20754242, 1.0),
        okhsl: Okhsla::new(328.3634, 0.99999994, 0.33011043, 1.0),
        okhsv: Okhsva::new(328.3634, 1.0, 0.5106205, 1.0),
        okhwb: Okhwba::new(328.3634, 0.0, 0.48937953, 1.0),
    },
    // aqua
    TestColor {
        name: "aqua",
        rgb: Srgba::new(0.0, 0.5, 0.5, 1.0),
        linear_rgb: LinearRgba::new(0.0, 0.21404114, 0.21404114, 1.0),
        hsl: Hsla::new(180.0, 1.0, 0.25, 1.0),
        hsv: Hsva::new(180.0, 1.0, 0.5, 1.0),
        hwb: Hwba::new(180.0, 0.0, 0.5, 1.0),
        lab: Laba::new(0.48073065, -0.28765038, -0.08452999, 1.0),
        lch: Lcha::new(0.48073065, 0.29981336, 196.37614, 1.0),
        oklab: Oklaba::new(0.54159236, -0.08939436, -0.02356726, 1.0),
        oklch: Oklcha::new(0.54159236, 0.09244873, 194.769, 1.0),
        xyz: Xyza::new(0.11515705, 0.16852042, 0.22891617, 1.0),
        okhsl: Okhsla::new(194.76895, 0.9999998, 0.46872336, 1.0),
        okhsv: Okhsva::new(194.76895, 0.9999998, 0.52782416, 1.0),
        okhwb: Okhwba::new(194.76895, 0.000000094382315, 0.47217584, 1.0),
    },
];

// -----------------------------------------------------------------------------
// Round-trip tests

#[test]
fn hsla_roundtrip() {
    for color in TEST_COLORS.iter() {
        let rgb2: Srgba = (color.hsl).into();
        let hsl2: Hsla = (color.rgb).into();
        assert!(
            color.rgb.distance(&rgb2) < 0.000001,
            "{}: {:?} != {:?}",
            color.name,
            color.rgb,
            rgb2
        );
        assert_approx_eq!(color.hsl.hue, hsl2.hue, 0.001);
        if color.name == "white_minus_epsilon" {
            // Our implementation differs from `palette`.
            // But it's OK because saturation doesn't matter when lightness is 1.0
            assert!(color.hsl.saturation != hsl2.saturation);
            assert_approx_eq!(color.hsl.lightness, 1.0, 0.001);
            assert_approx_eq!(1.0, hsl2.saturation, 0.001);
        } else {
            assert_approx_eq!(color.hsl.saturation, hsl2.saturation, 0.001);
        }
        assert_approx_eq!(color.hsl.lightness, hsl2.lightness, 0.001);
        assert_approx_eq!(color.hsl.alpha, hsl2.alpha, 0.001);
    }
}

#[test]
fn hsva_roundtrip() {
    for color in TEST_COLORS.iter() {
        let rgb2: Srgba = (color.hsv).into();
        let hsv2: Hsva = (color.rgb).into();
        assert!(
            color.rgb.distance(&rgb2) < 0.00001,
            "{}: {:?} != {:?}",
            color.name,
            color.rgb,
            rgb2
        );
        assert_approx_eq!(color.hsv.hue, hsv2.hue, 0.001);
        assert_approx_eq!(color.hsv.saturation, hsv2.saturation, 0.001);
        assert_approx_eq!(color.hsv.value, hsv2.value, 0.001);
        assert_approx_eq!(color.hsv.alpha, hsv2.alpha, 0.001);
    }
}

#[test]
fn hwba_roundtrip() {
    for color in TEST_COLORS.iter() {
        let rgb2: Srgba = (color.hwb).into();
        let hwb2: Hwba = (color.rgb).into();
        assert!(
            color.rgb.distance(&rgb2) < 0.00001,
            "{}: {:?} != {:?}",
            color.name,
            color.rgb,
            rgb2
        );
        assert_approx_eq!(color.hwb.hue, hwb2.hue, 0.001);
        assert_approx_eq!(color.hwb.whiteness, hwb2.whiteness, 0.001);
        assert_approx_eq!(color.hwb.blackness, hwb2.blackness, 0.001);
        assert_approx_eq!(color.hwb.alpha, hwb2.alpha, 0.001);
    }
}

#[test]
fn laba_roundtrip_srgba() {
    for color in TEST_COLORS.iter() {
        let rgb2: Srgba = (color.lab).into();
        let laba: Laba = (color.rgb).into();
        assert!(
            color.rgb.distance(&rgb2) < 0.0001,
            "{}: {:?} != {:?}",
            color.name,
            color.rgb,
            rgb2
        );
        assert_approx_eq!(color.lab.lightness, laba.lightness, 0.001);
        if laba.lightness > 0.01 {
            assert_approx_eq!(color.lab.a, laba.a, 0.1);
        }
        if laba.lightness > 0.01 && laba.a > 0.01 {
            assert!(
                ops::abs(color.lab.b - laba.b) < 1.7,
                "{:?} != {:?}",
                color.lab,
                laba
            );
        }
        assert_approx_eq!(color.lab.alpha, laba.alpha, 0.001);
    }
}

#[test]
fn laba_roundtrip_linear() {
    for color in TEST_COLORS.iter() {
        let rgb2: LinearRgba = (color.lab).into();
        let laba: Laba = (color.linear_rgb).into();
        assert!(
            color.linear_rgb.distance(&rgb2) < 0.0001,
            "{}: {:?} != {:?}",
            color.name,
            color.linear_rgb,
            rgb2
        );
        assert_approx_eq!(color.lab.lightness, laba.lightness, 0.001);
        if laba.lightness > 0.01 {
            assert_approx_eq!(color.lab.a, laba.a, 0.1);
        }
        if laba.lightness > 0.01 && laba.a > 0.01 {
            assert!(
                ops::abs(color.lab.b - laba.b) < 1.7,
                "{:?} != {:?}",
                color.lab,
                laba
            );
        }
        assert_approx_eq!(color.lab.alpha, laba.alpha, 0.001);
    }
}

#[test]
fn lcha_roundtrip_srgba() {
    for color in TEST_COLORS.iter() {
        let rgb2: Srgba = (color.lch).into();
        let lcha: Lcha = (color.rgb).into();
        assert!(
            color.rgb.distance(&rgb2) < 0.0001,
            "{}: {:?} != {:?}",
            color.name,
            color.rgb,
            rgb2
        );
        assert_approx_eq!(color.lch.lightness, lcha.lightness, 0.001);
        if lcha.lightness > 0.01 {
            assert_approx_eq!(color.lch.chroma, lcha.chroma, 0.1);
        }
        if lcha.lightness > 0.01 && lcha.chroma > 0.01 {
            assert!(
                ops::abs(color.lch.hue - lcha.hue) < 1.7,
                "{:?} != {:?}",
                color.lch,
                lcha
            );
        }
        assert_approx_eq!(color.lch.alpha, lcha.alpha, 0.001);
    }
}

#[test]
fn lcha_roundtrip_linear() {
    for color in TEST_COLORS.iter() {
        let rgb2: LinearRgba = (color.lch).into();
        let lcha: Lcha = (color.linear_rgb).into();
        assert!(
            color.linear_rgb.distance(&rgb2) < 0.0001,
            "{}: {:?} != {:?}",
            color.name,
            color.linear_rgb,
            rgb2
        );
        assert_approx_eq!(color.lch.lightness, lcha.lightness, 0.001);
        if lcha.lightness > 0.01 {
            assert_approx_eq!(color.lch.chroma, lcha.chroma, 0.1);
        }
        if lcha.lightness > 0.01 && lcha.chroma > 0.01 {
            assert!(
                ops::abs(color.lch.hue - lcha.hue) < 1.7,
                "{:?} != {:?}",
                color.lch,
                lcha
            );
        }
        assert_approx_eq!(color.lch.alpha, lcha.alpha, 0.001);
    }
}

#[test]
fn oklaba_roundtrip() {
    for color in TEST_COLORS.iter() {
        let rgb2: Srgba = (color.oklab).into();
        let oklab: Oklaba = (color.rgb).into();
        assert!(
            color.rgb.distance(&rgb2) < 0.0001,
            "{}: {:?} != {:?}",
            color.name,
            color.rgb,
            rgb2
        );
        assert!(
            color.oklab.distance(&oklab) < 0.0001,
            "{}: {:?} != {:?}",
            color.name,
            color.oklab,
            oklab
        );
    }
}

#[test]
fn oklcha_roundtrip() {
    for color in TEST_COLORS.iter() {
        let rgb2: Srgba = (color.oklch).into();
        let oklch: Oklcha = (color.rgb).into();
        assert!(
            color.rgb.distance(&rgb2) < 0.0001,
            "{}: {:?} != {:?}",
            color.name,
            color.rgb,
            rgb2
        );
        assert!(
            color.oklch.distance(&oklch) < 0.0001,
            "{}: {:?} != {:?}",
            color.name,
            color.oklch,
            oklch
        );
    }
}

#[test]
fn okhsla_roundtrip() {
    for color in TEST_COLORS.iter() {
        let rgb2: Srgba = (color.okhsl).into();
        let okhsl: Okhsla = (color.rgb).into();
        assert!(
            color.rgb.distance(&rgb2) < 0.001,
            "{}: {:?} != {:?}",
            color.name,
            color.rgb,
            rgb2
        );
        let msg = std::format!(
            "{}: expected {:?}, got {:?}",
            color.name,
            color.okhsl,
            okhsl
        );
        assert_approx_eq!(color.okhsl.hue, okhsl.hue, 0.001, msg);
        assert_approx_eq!(color.okhsl.saturation, okhsl.saturation, 0.001, msg);
        assert_approx_eq!(color.okhsl.lightness, okhsl.lightness, 0.001, msg);
        assert_approx_eq!(color.okhsl.alpha, okhsl.alpha, 0.001, msg);
    }
}

#[test]
fn okhsva_roundtrip() {
    for color in TEST_COLORS.iter() {
        let rgb2: Srgba = (color.okhsv).into();
        let okhsv: Okhsva = (color.rgb).into();
        assert!(
            color.rgb.distance(&rgb2) < 0.001,
            "{}: {:?} != {:?}",
            color.name,
            color.rgb,
            rgb2,
        );
        let msg = std::format!(
            "{}: expected {:?}, got {:?}",
            color.name,
            color.okhsv,
            okhsv
        );
        assert_approx_eq!(color.okhsv.hue, okhsv.hue, 0.001, msg);
        assert_approx_eq!(color.okhsv.saturation, okhsv.saturation, 0.001, msg);
        assert_approx_eq!(color.okhsv.value, okhsv.value, 0.001, msg);
        assert_approx_eq!(color.okhsv.alpha, okhsv.alpha, 0.001, msg);
    }
}

#[test]
fn okhwba_roundtrip() {
    for color in TEST_COLORS.iter() {
        let rgb2: Srgba = (color.okhwb).into();
        let okhwb: Okhwba = (color.rgb).into();
        assert!(
            color.rgb.distance(&rgb2) < 0.001,
            "{}: {:?} != {:?}",
            color.name,
            color.rgb,
            rgb2,
        );
        let msg = std::format!(
            "{}: expected {:?}, got {:?}",
            color.name,
            color.okhwb,
            okhwb
        );
        assert_approx_eq!(color.okhwb.hue, okhwb.hue, 0.001, msg);
        assert_approx_eq!(color.okhwb.whiteness, okhwb.whiteness, 0.001, msg);
        assert_approx_eq!(color.okhwb.blackness, okhwb.blackness, 0.001, msg);
        assert_approx_eq!(color.okhwb.alpha, okhwb.alpha, 0.001, msg);
    }
}

#[test]
fn xyza_roundtrip() {
    for color in TEST_COLORS.iter() {
        let rgb2: Srgba = (color.xyz).into();
        let xyz2: Xyza = (color.rgb).into();
        assert!(
            color.rgb.distance(&rgb2) < 0.00001,
            "{}: {:?} != {:?}",
            color.name,
            color.rgb,
            rgb2
        );
        assert_approx_eq!(color.xyz.x, xyz2.x, 0.001);
        assert_approx_eq!(color.xyz.y, xyz2.y, 0.001);
        assert_approx_eq!(color.xyz.z, xyz2.z, 0.001);
        assert_approx_eq!(color.xyz.alpha, xyz2.alpha, 0.001);
    }
}

// -----------------------------------------------------------------------------
// Per-space conversion tests

#[test]
fn hsla_to_from_srgba() {
    let hsla = Hsla::new(0.5, 0.5, 0.5, 1.0);
    let srgba: Srgba = hsla.into();
    let hsla2: Hsla = srgba.into();
    assert_approx_eq!(hsla.hue, hsla2.hue, 0.001);
    assert_approx_eq!(hsla.saturation, hsla2.saturation, 0.001);
    assert_approx_eq!(hsla.lightness, hsla2.lightness, 0.001);
    assert_approx_eq!(hsla.alpha, hsla2.alpha, 0.001);
}

#[test]
fn hsla_to_from_linear() {
    let hsla = Hsla::new(0.5, 0.5, 0.5, 1.0);
    let linear: LinearRgba = hsla.into();
    let hsla2: Hsla = linear.into();
    assert_approx_eq!(hsla.hue, hsla2.hue, 0.001);
    assert_approx_eq!(hsla.saturation, hsla2.saturation, 0.001);
    assert_approx_eq!(hsla.lightness, hsla2.lightness, 0.001);
    assert_approx_eq!(hsla.alpha, hsla2.alpha, 0.001);
}

#[test]
fn hsla_mix_wrap() {
    let hsla0 = Hsla::new(10., 0.5, 0.5, 1.0);
    let hsla1 = Hsla::new(20., 0.5, 0.5, 1.0);
    let hsla2 = Hsla::new(350., 0.5, 0.5, 1.0);
    assert_approx_eq!(hsla0.mix(&hsla1, 0.25).hue, 12.5, 0.001);
    assert_approx_eq!(hsla0.mix(&hsla1, 0.5).hue, 15., 0.001);
    assert_approx_eq!(hsla0.mix(&hsla1, 0.75).hue, 17.5, 0.001);

    assert_approx_eq!(hsla1.mix(&hsla0, 0.25).hue, 17.5, 0.001);
    assert_approx_eq!(hsla1.mix(&hsla0, 0.5).hue, 15., 0.001);
    assert_approx_eq!(hsla1.mix(&hsla0, 0.75).hue, 12.5, 0.001);

    assert_approx_eq!(hsla0.mix(&hsla2, 0.25).hue, 5., 0.001);
    assert_approx_eq!(hsla0.mix(&hsla2, 0.5).hue, 0., 0.001);
    assert_approx_eq!(hsla0.mix(&hsla2, 0.75).hue, 355., 0.001);

    assert_approx_eq!(hsla2.mix(&hsla0, 0.25).hue, 355., 0.001);
    assert_approx_eq!(hsla2.mix(&hsla0, 0.5).hue, 0., 0.001);
    assert_approx_eq!(hsla2.mix(&hsla0, 0.75).hue, 5., 0.001);
}

#[test]
fn hsla_from_index() {
    let references = [
        Hsla::hsl(0.0, 1., 0.5),
        Hsla::hsl(222.49225, 1., 0.5),
        Hsla::hsl(84.984474, 1., 0.5),
        Hsla::hsl(307.4767, 1., 0.5),
        Hsla::hsl(169.96895, 1., 0.5),
    ];

    for (index, reference) in references.into_iter().enumerate() {
        let color = Hsla::sequential_dispersed(index as u32);

        assert_approx_eq!(color.hue, reference.hue, 0.001);
    }
}

#[test]
fn hsva_to_from_srgba() {
    let hsva = Hsva::new(180., 0.5, 0.5, 1.0);
    let srgba: Srgba = hsva.into();
    let hsva2: Hsva = srgba.into();
    assert_approx_eq!(hsva.hue, hsva2.hue, 0.001);
    assert_approx_eq!(hsva.saturation, hsva2.saturation, 0.001);
    assert_approx_eq!(hsva.value, hsva2.value, 0.001);
    assert_approx_eq!(hsva.alpha, hsva2.alpha, 0.001);
}

#[test]
fn hwba_to_from_srgba() {
    let hwba = Hwba::new(0.0, 0.5, 0.5, 1.0);
    let srgba: Srgba = hwba.into();
    let hwba2: Hwba = srgba.into();
    assert_approx_eq!(hwba.hue, hwba2.hue, 0.001);
    assert_approx_eq!(hwba.whiteness, hwba2.whiteness, 0.001);
    assert_approx_eq!(hwba.blackness, hwba2.blackness, 0.001);
    assert_approx_eq!(hwba.alpha, hwba2.alpha, 0.001);
}

#[test]
fn lcha_wide_gamut_chroma_preserved() {
    let laba = Laba::new(0.8, 1.5, -1.2, 1.0);
    let lcha: Lcha = laba.into();
    assert!(lcha.chroma > 1.9, "chroma was clamped: {:?}", lcha);

    let back: Laba = lcha.into();
    assert_approx_eq!(laba.lightness, back.lightness, 1e-4);
    assert_approx_eq!(laba.a, back.a, 1e-4);
    assert_approx_eq!(laba.b, back.b, 1e-4);
}

#[test]
fn oklaba_to_from_srgba() {
    let oklaba = Oklaba::new(0.5, 0.5, 0.5, 1.0);
    let srgba: Srgba = oklaba.into();
    let oklaba2: Oklaba = srgba.into();
    assert_approx_eq!(oklaba.lightness, oklaba2.lightness, 0.001);
    assert_approx_eq!(oklaba.a, oklaba2.a, 0.001);
    assert_approx_eq!(oklaba.b, oklaba2.b, 0.001);
    assert_approx_eq!(oklaba.alpha, oklaba2.alpha, 0.001);
}

#[test]
fn oklaba_to_from_linear() {
    let oklaba = Oklaba::new(0.5, 0.5, 0.5, 1.0);
    let linear: LinearRgba = oklaba.into();
    let oklaba2: Oklaba = linear.into();
    assert_approx_eq!(oklaba.lightness, oklaba2.lightness, 0.001);
    assert_approx_eq!(oklaba.a, oklaba2.a, 0.001);
    assert_approx_eq!(oklaba.b, oklaba2.b, 0.001);
    assert_approx_eq!(oklaba.alpha, oklaba2.alpha, 0.001);
}

#[test]
fn oklcha_to_from_srgba() {
    let oklcha = Oklcha::new(0.5, 0.5, 180.0, 1.0);
    let srgba: Srgba = oklcha.into();
    let oklcha2: Oklcha = srgba.into();
    assert_approx_eq!(oklcha.lightness, oklcha2.lightness, 0.001);
    assert_approx_eq!(oklcha.chroma, oklcha2.chroma, 0.001);
    assert_approx_eq!(oklcha.hue, oklcha2.hue, 0.001);
    assert_approx_eq!(oklcha.alpha, oklcha2.alpha, 0.001);
}

#[test]
fn oklcha_to_from_linear() {
    let oklcha = Oklcha::new(0.5, 0.5, 0.5, 1.0);
    let linear: LinearRgba = oklcha.into();
    let oklcha2: Oklcha = linear.into();
    assert_approx_eq!(oklcha.lightness, oklcha2.lightness, 0.001);
    assert_approx_eq!(oklcha.chroma, oklcha2.chroma, 0.001);
    assert_approx_eq!(oklcha.hue, oklcha2.hue, 0.001);
    assert_approx_eq!(oklcha.alpha, oklcha2.alpha, 0.001);
}

#[test]
fn okhsla_from_to_oklaba() {
    // Test `oklab_l == 0.0`
    let oklaba = Oklaba::new(0.0, 0.5, 0.5, 1.0);
    let okhsla: Okhsla = oklaba.into();
    let oklaba2: Oklaba = okhsla.into();
    assert_approx_eq!(okhsla.hue, 0.0, 0.001);
    assert_approx_eq!(okhsla.saturation, 0.0, 0.001);
    assert_approx_eq!(okhsla.lightness, 0.0, 0.001);
    assert_approx_eq!(okhsla.alpha, 1.0, 0.001);

    assert_approx_eq!(oklaba.lightness, oklaba2.lightness, 0.001);
    assert_approx_eq!(0.0, oklaba2.a, 0.001);
    assert_approx_eq!(0.0, oklaba2.b, 0.001);
    assert_approx_eq!(oklaba.alpha, oklaba2.alpha, 0.001);

    // Test `oklab_l == 1.0`
    let oklaba = Oklaba::new(1.0, 0.5, 0.5, 1.0);
    let okhsla: Okhsla = oklaba.into();
    let oklaba2: Oklaba = okhsla.into();
    assert_approx_eq!(okhsla.hue, 0.0, 0.001);
    assert_approx_eq!(okhsla.saturation, 0.0, 0.001);
    assert_approx_eq!(okhsla.lightness, 1.0, 0.001);
    assert_approx_eq!(okhsla.alpha, 1.0, 0.001);

    assert_approx_eq!(oklaba.lightness, oklaba2.lightness, 0.001);
    assert_approx_eq!(0.0, oklaba2.a, 0.001);
    assert_approx_eq!(0.0, oklaba2.b, 0.001);
    assert_approx_eq!(oklaba.alpha, oklaba2.alpha, 0.001);

    // Test `oklab_a == 0.0 && oklab_b ==0.0` (C == 0.0)
    let oklaba = Oklaba::new(0.5, 0.0, 0.0, 1.0);
    let okhsla: Okhsla = oklaba.into();
    let oklaba2: Oklaba = okhsla.into();
    assert_approx_eq!(okhsla.hue, 0.0, 0.001);
    assert_approx_eq!(okhsla.saturation, 0.0, 0.001);
    assert_approx_eq!(okhsla.lightness, 0.42114055, 0.001);
    assert_approx_eq!(okhsla.alpha, 1.0, 0.001);

    assert_approx_eq!(oklaba.lightness, oklaba2.lightness, 0.001);
    assert_approx_eq!(0.0, oklaba2.a, 0.001);
    assert_approx_eq!(0.0, oklaba2.b, 0.001);
    assert_approx_eq!(oklaba.alpha, oklaba2.alpha, 0.001);
}

#[test]
fn okhsla_to_from_srgba() {
    let okhsla = Okhsla::new(180.0, 0.5, 0.5, 1.0);
    let srgba: Srgba = okhsla.into();
    let okhsla2: Okhsla = srgba.into();
    assert_approx_eq!(okhsla.hue, okhsla2.hue, 0.001);
    assert_approx_eq!(okhsla.saturation, okhsla2.saturation, 0.001);
    assert_approx_eq!(okhsla.lightness, okhsla2.lightness, 0.001);
    assert_approx_eq!(okhsla.alpha, okhsla2.alpha, 0.001);
}

#[test]
fn okhsla_to_from_linear() {
    let okhsla = Okhsla::new(180.0, 0.5, 0.5, 1.0);
    let linear: LinearRgba = okhsla.into();
    let okhsla2: Okhsla = linear.into();
    assert_approx_eq!(okhsla.hue, okhsla2.hue, 0.001);
    assert_approx_eq!(okhsla.saturation, okhsla2.saturation, 0.001);
    assert_approx_eq!(okhsla.lightness, okhsla2.lightness, 0.001);
    assert_approx_eq!(okhsla.alpha, okhsla2.alpha, 0.001);
}

#[test]
fn okhsva_from_oklaba() {
    // Test `oklab_l == 0.0`
    let oklaba = Oklaba::new(0.0, 0.5, 0.5, 1.0);
    let okhsva: Okhsva = oklaba.into();
    let oklaba2: Oklaba = okhsva.into();
    assert_approx_eq!(okhsva.hue, 0.0, 0.001);
    assert_approx_eq!(okhsva.saturation, 0.0, 0.001);
    assert_approx_eq!(okhsva.value, 0.0, 0.001);
    assert_approx_eq!(okhsva.alpha, 1.0, 0.001);

    assert_approx_eq!(oklaba.lightness, oklaba2.lightness, 0.001);
    assert_approx_eq!(0.0, oklaba2.a, 0.001);
    assert_approx_eq!(0.0, oklaba2.b, 0.001);
    assert_approx_eq!(oklaba.alpha, oklaba2.alpha, 0.001);

    // Test `oklab_l == 1.0`
    let oklaba = Oklaba::new(1.0, 0.5, 0.5, 1.0);
    let okhsva: Okhsva = oklaba.into();
    let oklaba2: Oklaba = okhsva.into();
    assert_approx_eq!(okhsva.hue, 0.0, 0.001);
    assert_approx_eq!(okhsva.saturation, 0.0, 0.001);
    assert_approx_eq!(okhsva.value, 1.0, 0.001);
    assert_approx_eq!(okhsva.alpha, 1.0, 0.001);

    assert_approx_eq!(oklaba.lightness, oklaba2.lightness, 0.001);
    assert_approx_eq!(0.0, oklaba2.a, 0.001);
    assert_approx_eq!(0.0, oklaba2.b, 0.001);
    assert_approx_eq!(oklaba.alpha, oklaba2.alpha, 0.001);

    // Test `oklab_a == 0.0 && oklab_b ==0.0` (C == 0.0)
    let oklaba = Oklaba::new(0.5, 0.0, 0.0, 1.0);
    let okhsva: Okhsva = oklaba.into();
    let oklaba2: Oklaba = okhsva.into();
    assert_approx_eq!(okhsva.hue, 0.0, 0.001);
    assert_approx_eq!(okhsva.saturation, 0.0, 0.001);
    assert_approx_eq!(okhsva.value, 0.42114055, 0.001);
    assert_approx_eq!(okhsva.alpha, 1.0, 0.001);

    assert_approx_eq!(oklaba.lightness, oklaba2.lightness, 0.001);
    assert_approx_eq!(0.0, oklaba2.a, 0.001);
    assert_approx_eq!(0.0, oklaba2.b, 0.001);
    assert_approx_eq!(oklaba.alpha, oklaba2.alpha, 0.001);
}

#[test]
fn okhsva_to_from_srgba() {
    let okhsva = Okhsva::new(180.0, 0.5, 0.5, 1.0);
    let srgba: Srgba = okhsva.into();
    let okhsva2: Okhsva = srgba.into();
    assert_approx_eq!(okhsva.hue, okhsva2.hue, 0.001);
    assert_approx_eq!(okhsva.saturation, okhsva2.saturation, 0.001);
    assert_approx_eq!(okhsva.value, okhsva2.value, 0.001);
    assert_approx_eq!(okhsva.alpha, okhsva2.alpha, 0.001);
}

#[test]
fn okhsva_to_from_linear() {
    let okhsva = Okhsva::new(0.5, 0.5, 0.5, 1.0);
    let linear: LinearRgba = okhsva.into();
    let okhsva2: Okhsva = linear.into();
    assert_approx_eq!(okhsva.hue, okhsva2.hue, 0.001);
    assert_approx_eq!(okhsva.saturation, okhsva2.saturation, 0.001);
    assert_approx_eq!(okhsva.value, okhsva2.value, 0.001);
    assert_approx_eq!(okhsva.alpha, okhsva2.alpha, 0.001);
}

#[test]
fn okhwba_from_okhsva() {
    // Test `saturation == 0.0`
    let okhsva = Okhsva::new(90.0, 0.0, 0.4, 1.0);
    let okhwba: Okhwba = okhsva.into();
    let okhsva2: Okhsva = okhwba.into();
    assert_approx_eq!(okhwba.hue, 90.0, 0.001);
    assert_approx_eq!(okhwba.whiteness, 0.4, 0.001);
    assert_approx_eq!(okhwba.blackness, 0.6, 0.001);
    assert_approx_eq!(okhwba.alpha, 1.0, 0.001);

    assert_approx_eq!(okhsva.hue, okhsva2.hue, 0.001);
    assert_approx_eq!(okhsva.saturation, okhsva2.saturation, 0.001);
    assert_approx_eq!(okhsva.value, okhsva2.value, 0.001);
    assert_approx_eq!(okhsva.alpha, okhsva2.alpha, 0.001);

    // Test `saturation == 1.0 && value == 1.0`
    let okhsva = Okhsva::new(270.0, 1.0, 1.0, 1.0);
    let okhwba: Okhwba = okhsva.into();
    let okhsva2: Okhsva = okhwba.into();
    assert_approx_eq!(okhwba.hue, 270.0, 0.001);
    assert_approx_eq!(okhwba.whiteness, 0.0, 0.001);
    assert_approx_eq!(okhwba.blackness, 0.0, 0.001);
    assert_approx_eq!(okhwba.alpha, 1.0, 0.001);

    assert_approx_eq!(okhsva.hue, okhsva2.hue, 0.001);
    assert_approx_eq!(okhsva.saturation, okhsva2.saturation, 0.001);
    assert_approx_eq!(okhsva.value, okhsva2.value, 0.001);
    assert_approx_eq!(okhsva.alpha, okhsva2.alpha, 0.001);

    // Test `saturation == 0.0 && value == 1.0` (white)
    let okhsva = Okhsva::new(0.0, 0.0, 1.0, 1.0);
    let okhwba: Okhwba = okhsva.into();
    let okhsva2: Okhsva = okhwba.into();
    assert_approx_eq!(okhwba.hue, 0.0, 0.001);
    assert_approx_eq!(okhwba.whiteness, 1.0, 0.001);
    assert_approx_eq!(okhwba.blackness, 0.0, 0.001);
    assert_approx_eq!(okhwba.alpha, 1.0, 0.001);

    assert_approx_eq!(okhsva.hue, okhsva2.hue, 0.001);
    assert_approx_eq!(okhsva.saturation, okhsva2.saturation, 0.001);
    assert_approx_eq!(okhsva.value, okhsva2.value, 0.001);
    assert_approx_eq!(okhsva.alpha, okhsva2.alpha, 0.001);

    // Test `value == 0.0` (black)
    let okhsva = Okhsva::new(0.0, 0.8, 0.0, 1.0);
    let okhwba: Okhwba = okhsva.into();
    let okhsva2: Okhsva = okhwba.into();
    assert_approx_eq!(okhwba.hue, 0.0, 0.001);
    assert_approx_eq!(okhwba.whiteness, 0.0, 0.001);
    assert_approx_eq!(okhwba.blackness, 1.0, 0.001);
    assert_approx_eq!(okhwba.alpha, 1.0, 0.001);

    assert_approx_eq!(okhsva.hue, okhsva2.hue, 0.001);
    assert_approx_eq!(0.0, okhsva2.saturation, 0.001);
    assert_approx_eq!(okhsva.value, okhsva2.value, 0.001);
    assert_approx_eq!(okhsva.alpha, okhsva2.alpha, 0.001);
}

#[test]
fn okhwba_from_oklaba() {
    // Test `oklab_l == 0.0`
    let oklaba = Oklaba::new(0.0, 0.5, 0.5, 1.0);
    let okhwba: Okhwba = oklaba.into();
    let oklaba2: Oklaba = okhwba.into();
    assert_approx_eq!(okhwba.hue, 0.0, 0.001);
    assert_approx_eq!(okhwba.whiteness, 0.0, 0.001);
    assert_approx_eq!(okhwba.blackness, 1.0, 0.001);
    assert_approx_eq!(okhwba.alpha, 1.0, 0.001);

    assert_approx_eq!(oklaba.lightness, oklaba2.lightness, 0.001);
    assert_approx_eq!(0.0, oklaba2.a, 0.001);
    assert_approx_eq!(0.0, oklaba2.b, 0.001);
    assert_approx_eq!(oklaba.alpha, oklaba2.alpha, 0.001);

    // Test `oklab_l == 1.0`
    let oklaba = Oklaba::new(1.0, 0.5, 0.5, 1.0);
    let okhwba: Okhwba = oklaba.into();
    let oklaba2: Oklaba = okhwba.into();
    assert_approx_eq!(okhwba.hue, 0.0, 0.001);
    assert_approx_eq!(okhwba.whiteness, 1.0, 0.001);
    assert_approx_eq!(okhwba.blackness, 0.0, 0.001);
    assert_approx_eq!(okhwba.alpha, 1.0, 0.001);

    assert_approx_eq!(oklaba.lightness, oklaba2.lightness, 0.001);
    assert_approx_eq!(0.0, oklaba2.a, 0.001);
    assert_approx_eq!(0.0, oklaba2.b, 0.001);
    assert_approx_eq!(oklaba.alpha, oklaba2.alpha, 0.001);

    // Test `oklab_a == 0.0 && oklab_b ==0.0` (C == 0.0)
    let oklaba = Oklaba::new(0.5, 0.0, 0.0, 1.0);
    let okhwba: Okhwba = oklaba.into();
    let oklaba2: Oklaba = okhwba.into();
    assert_approx_eq!(okhwba.hue, 0.0, 0.001);
    assert_approx_eq!(okhwba.whiteness, 0.42114055, 0.001);
    assert_approx_eq!(okhwba.blackness, 0.57885945, 0.001);
    assert_approx_eq!(okhwba.alpha, 1.0, 0.001);

    assert_approx_eq!(oklaba.lightness, oklaba2.lightness, 0.001);
    assert_approx_eq!(0.0, oklaba2.a, 0.001);
    assert_approx_eq!(0.0, oklaba2.b, 0.001);
    assert_approx_eq!(oklaba.alpha, oklaba2.alpha, 0.001);
}

#[test]
fn okhwba_to_from_srgba() {
    let okhwba = Okhwba::new(180.0, 0.5, 0.5, 1.0);
    let srgba: Srgba = okhwba.into();
    let okhwba2: Okhwba = srgba.into();
    assert_approx_eq!(okhwba.hue, okhwba2.hue, 0.001);
    assert_approx_eq!(okhwba.whiteness, okhwba2.whiteness, 0.001);
    assert_approx_eq!(okhwba.blackness, okhwba2.blackness, 0.001);
    assert_approx_eq!(okhwba.alpha, okhwba2.alpha, 0.001);
}

#[test]
fn okhwba_to_from_linear() {
    let okhwba = Okhwba::new(180.0, 0.5, 0.5, 1.0);
    let linear: LinearRgba = okhwba.into();
    let okhwba2: Okhwba = linear.into();
    assert_approx_eq!(okhwba.hue, okhwba2.hue, 0.001);
    assert_approx_eq!(okhwba.whiteness, okhwba2.whiteness, 0.001);
    assert_approx_eq!(okhwba.blackness, okhwba2.blackness, 0.001);
    assert_approx_eq!(okhwba.alpha, okhwba2.alpha, 0.001);
}

#[test]
fn xyza_to_from_srgba() {
    let xyza = Xyza::new(0.5, 0.5, 0.5, 1.0);
    let srgba: Srgba = xyza.into();
    let xyza2: Xyza = srgba.into();
    assert_approx_eq!(xyza.x, xyza2.x, 0.001);
    assert_approx_eq!(xyza.y, xyza2.y, 0.001);
    assert_approx_eq!(xyza.z, xyza2.z, 0.001);
    assert_approx_eq!(xyza.alpha, xyza2.alpha, 0.001);
}
