use zlim_reg::*;

#[derive(Debug, Clone, Copy)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Int(u32);

collect!(Int);

submit!(Int(10) => Int as ITEM);

#[test]
fn is_submitted() {
    assert!(ITEM.is_submitted());
}

#[test]
fn submit_and_iter() {
    submit!(Int(1) => Int);
    submit!(Int(2) => Int);
    submit!(Int(3) => Int);

    const EXPECT: &[Int] = &[Int(1), Int(2), Int(3), Int(10)];

    let mut r1: Vec<Int> = iter::<Int>().copied().collect();
    let mut r2: Vec<Int> = iter::<Int>().copied().collect();
    r1.sort();
    r2.sort();
    assert_eq!(r1, r2);
    assert_eq!(r1.as_slice(), EXPECT);
}
