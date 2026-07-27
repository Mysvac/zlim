/// Call the target macro and pass a sequence of numbers as parameters.
///
/// The number cannot exceed `15`.
///
/// # Examples
///
/// ```ignore
/// range_invoke!(my_macro,  4);
/// // ↓
/// my_macro!(0: []);
/// my_macro!(1: [0: P0]);
/// my_macro!(2: [0: P0, 1: P1]);
/// my_macro!(3: [0: P0, 1: P1, 2: P2]);
/// my_macro!(4: [0: P0, 1: P1, 2: P2, 3: P3]);
/// ```
#[macro_export]
macro_rules! range_invoke {
    ($macro:ident, 0) => {
        $macro!(0: []);
    };
    ($macro:ident, 1) => {
        $macro!(0: []);
        $macro!(1: [0: P0]);
    };
    ($macro:ident, 2) => {
        $macro!(0: []);
        $macro!(1: [0: P0]);
        $macro!(2: [0: P0, 1: P1]);
    };
    ($macro:ident, 3) => {
        $crate::range_invoke!($macro, 2);
        $macro!(3: [0: P0, 1: P1, 2: P2]);
    };
    ($macro:ident, 4) => {
        $crate::range_invoke!($macro, 2);
        $macro!(3: [0: P0, 1: P1, 2: P2]);
        $macro!(4: [0: P0, 1: P1, 2: P2, 3: P3]);
    };
    ($macro:ident, 5) => {
        $crate::range_invoke!($macro, 2);
        $macro!(3: [0: P0, 1: P1, 2: P2]);
        $macro!(4: [0: P0, 1: P1, 2: P2, 3: P3]);
        $macro!(5: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4]);
    };
    ($macro:ident, 6) => {
        $crate::range_invoke!($macro, 5);
        $macro!(6: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5]);
    };
    ($macro:ident, 7) => {
        $crate::range_invoke!($macro, 5);
        $macro!(6: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5]);
        $macro!(7: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5, 6: P6]);
    };
    ($macro:ident, 8) => {
        $crate::range_invoke!($macro, 5);
        $macro!(6: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5]);
        $macro!(7: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5, 6: P6]);
        $macro!(8: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5, 6: P6, 7: P7]);
    };
    ($macro:ident, 9) => {
        $crate::range_invoke!($macro, 8);
        $macro!(9: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5, 6: P6, 7: P7, 8: P8]);
    };
    ($macro:ident, 10) => {
        $crate::range_invoke!($macro, 8);
        $macro!(9: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5, 6: P6, 7: P7, 8: P8]);
        $macro!(10: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5, 6: P6, 7: P7, 8: P8, 9: P9]);
    };
    ($macro:ident, 11) => {
        $crate::range_invoke!($macro, 8);
        $macro!(9: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5, 6: P6, 7: P7, 8: P8]);
        $macro!(10: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5, 6: P6, 7: P7, 8: P8, 9: P9]);
        $macro!(11: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5, 6: P6, 7: P7, 8: P8, 9: P9, 10: P10]);
    };
    ($macro:ident, 12) => {
        $crate::range_invoke!($macro, 8);
        $macro!(9: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5, 6: P6, 7: P7, 8: P8]);
        $macro!(10: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5, 6: P6, 7: P7, 8: P8, 9: P9]);
        $macro!(11: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5, 6: P6, 7: P7, 8: P8, 9: P9, 10: P10]);
        $macro!(12: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5, 6: P6, 7: P7, 8: P8, 9: P9, 10: P10, 11: P11]);
    };
    ($macro:ident, 13) => {
        $crate::range_invoke!($macro, 12);
        $macro!(13: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5, 6: P6, 7: P7, 8: P8, 9: P9, 10: P10, 11: P11, 12: P12]);
    };
    ($macro:ident, 14) => {
        $crate::range_invoke!($macro, 12);
        $macro!(13: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5, 6: P6, 7: P7, 8: P8, 9: P9, 10: P10, 11: P11, 12: P12]);
        $macro!(14: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5, 6: P6, 7: P7, 8: P8, 9: P9, 10: P10, 11: P11, 12: P12, 13: P13]);
    };
    ($macro:ident, 15) => {
        $crate::range_invoke!($macro, 12);
        $macro!(13: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5, 6: P6, 7: P7, 8: P8, 9: P9, 10: P10, 11: P11, 12: P12]);
        $macro!(14: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5, 6: P6, 7: P7, 8: P8, 9: P9, 10: P10, 11: P11, 12: P12, 13: P13]);
        $macro!(15: [0: P0, 1: P1, 2: P2, 3: P3, 4: P4, 5: P5, 6: P6, 7: P7, 8: P8, 9: P9, 10: P10, 11: P11, 12: P12, 13: P13, 14: P14]);
    };
}

/// Call the target macro and pass a sequence of numbers as parameters.
///
/// The number cannot exceed `8`.
///
/// # Examples
///
/// ```ignore
/// range_invoke2!(my_macro,  4);
/// // ↓
/// my_macro!(0: []);
/// my_macro!(1: [0: P0 T0]);
/// my_macro!(2: [0: P0 T0, 1: P1 T1]);
/// my_macro!(3: [0: P0 T0, 1: P1 T1, 2: P2 T2]);
/// my_macro!(4: [0: P0 T0, 1: P1 T1, 2: P2 T2, 3: P3 T3]);
/// ```
#[macro_export]
macro_rules! range_invoke2 {
    ($macro:ident, 0) => {
        $macro!(0: []);
    };
    ($macro:ident, 1) => {
        $macro!(0: []);
        $macro!(1: [0: P0 T0]);
    };
    ($macro:ident, 2) => {
        $macro!(0: []);
        $macro!(1: [0: P0 T0]);
        $macro!(2: [0: P0 T0, 1: P1 T1]);
    };
    ($macro:ident, 3) => {
        $crate::range_invoke2!($macro, 2);
        $macro!(3: [0: P0 T0, 1: P1 T1, 2: P2 T2]);
    };
    ($macro:ident, 4) => {
        $crate::range_invoke2!($macro, 2);
        $macro!(3: [0: P0 T0, 1: P1 T1, 2: P2 T2]);
        $macro!(4: [0: P0 T0, 1: P1 T1, 2: P2 T2, 3: P3 T3]);
    };
    ($macro:ident, 5) => {
        $crate::range_invoke2!($macro, 2);
        $macro!(3: [0: P0 T0, 1: P1 T1, 2: P2 T2]);
        $macro!(4: [0: P0 T0, 1: P1 T1, 2: P2 T2, 3: P3 T3]);
        $macro!(5: [0: P0 T0, 1: P1 T1, 2: P2 T2, 3: P3 T3, 4: P4 T4]);
    };
    ($macro:ident, 6) => {
        $crate::range_invoke2!($macro, 5);
        $macro!(6: [0: P0 T0, 1: P1 T1, 2: P2 T2, 3: P3 T3, 4: P4 T4, 5: P5 T5]);
    };
    ($macro:ident, 7) => {
        $crate::range_invoke2!($macro, 5);
        $macro!(6: [0: P0 T0, 1: P1 T1, 2: P2 T2, 3: P3 T3, 4: P4 T4, 5: P5 T5]);
        $macro!(7: [0: P0 T0, 1: P1 T1, 2: P2 T2, 3: P3 T3, 4: P4 T4, 5: P5 T5, 6: P6 T6]);
    };
    ($macro:ident, 8) => {
        $crate::range_invoke2!($macro, 5);
        $macro!(6: [0: P0 T0, 1: P1 T1, 2: P2 T2, 3: P3 T3, 4: P4 T4, 5: P5 T5]);
        $macro!(7: [0: P0 T0, 1: P1 T1, 2: P2 T2, 3: P3 T3, 4: P4 T4, 5: P5 T5, 6: P6 T6]);
        $macro!(8: [0: P0 T0, 1: P1 T1, 2: P2 T2, 3: P3 T3, 4: P4 T4, 5: P5 T5, 6: P6 T6, 7: P7 T7]);
    };
}
