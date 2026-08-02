#![doc = include_str!("../README.md")]
#![expect(unsafe_code, reason = "pointer operation")]
#![no_std]

use core::any::TypeId;
use core::cell::UnsafeCell;
use core::iter::FusedIterator;
use core::marker::PhantomData;
use core::ptr;
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

const PENDING: usize = 0;
const RUNNING: usize = 1;
const COMPLETED: usize = 2;

// ----------------------------------------------------------------------------
// Node

/// A type erased node in registry linked-list.
struct Node {
    // Type-erased pointer to static data.
    data: *const (),
    // `next` is written only during submission, and exclusivity
    // is ensured by the atomic submission state. Before `submit`
    // completes, `next` is never read by iterators, so `next`
    // itself does not need to be atomic.
    next: UnsafeCell<Option<&'static Node>>,
    // We require the target platform to support `AtomicPtr`, but not necessarily
    // `AtomicU8`. So we use `AtomicUsize` instead, which also keeps struct size the same.
    state: AtomicUsize,
}

// ----------------------------------------------------------------------------
// Item

/// A registrable inventory entry.
///
/// This type is intentionally public so users can opt into
/// manual submission:
///
/// - Create an item with the [`submit!`] macro using the
///   syntax `value => type as ident`.
/// - Check if already submitted with [`Item::is_submitted`].
/// - Manually submit with [`Item::submit`].
///
/// In most cases, users will not need to interact with this type directly.
#[repr(transparent)]
pub struct Item<T> {
    node: Node,
    _marker: PhantomData<T>,
}

// ----------------------------------------------------------------------------
// Registry

/// Registry storage for one inventory type.
///
/// Internally this is a singly linked list head.
///
/// Reusing one [`Registry`] for multiple concrete types
/// is undefined behavior.
pub struct Registry {
    // Head pointer of the singly linked list for one concrete type.
    // New entries are inserted at the head (push-front).
    head: AtomicPtr<Node>,
    // Ensure type correctness
    type_id: TypeId,
}

// ----------------------------------------------------------------------------
// Iter

/// Iterator over all submitted values of type `T`.
///
/// The iteration order is unspecified.
///
/// Construct this iterator through [`iter`].
#[repr(transparent)]
pub struct Iter<T> {
    node: Option<&'static Node>,
    _marker: PhantomData<T>,
}

// ----------------------------------------------------------------------------
// Collect

/// Marker trait for types that can participate in this inventory.
///
/// Prefer implementing this trait through [`collect!`].
///
/// # Examples
///
/// ```no_run
/// # use zlim_reg::{Collect, Registry};
///
/// struct Flag;
///
/// impl Collect for Flag {
///     fn registry() -> &'static Registry {
///         static REG: Registry = Registry::new::<Flag>();
///         &REG
///     }
/// }
/// ```
///
/// # Safety
///
/// The returned registry must be dedicated to exactly one concrete type.
///
/// For example, avoid patterns that may cause the same [`Registry`] instance
/// to be shared across unrelated types, which would corrupt internal typing and
/// may trigger undefined behavior during iteration.
#[diagnostic::on_unimplemented(
    message = "`{Self}` does not implement `Collect`",
    label = "`{Self}` does not implement `Collect`",
    note = "consider using `zlim_reg::collect!({Self});`"
)]
pub trait Collect: Sync + Sized + 'static {
    fn registry() -> &'static Registry;
}

// ----------------------------------------------------------------------------
// Implementation

impl Registry {
    /// Creates an empty registry for a specific type.
    pub const fn new<T: 'static>() -> Self {
        Self {
            head: AtomicPtr::new(ptr::null_mut()),
            type_id: TypeId::of::<T>(),
        }
    }
}

unsafe impl<T: Collect> Sync for Item<T> {}

impl<T: Collect> Item<T> {
    /// Creates a registrable item that points to a `'static` value.
    pub const fn new(val: &'static T) -> Self {
        Self {
            _marker: PhantomData,
            node: Node {
                data: val as *const T as *const (),
                next: UnsafeCell::new(None),
                state: AtomicUsize::new(PENDING),
            },
        }
    }

    /// Returns whether this item has already been submitted.
    pub fn is_submitted(&self) -> bool {
        self.node.state.load(Ordering::Acquire) == COMPLETED
    }

    /// Submits this item into `T`'s registry.
    ///
    /// Repeated calls are idempotent.
    #[inline(never)] // Ensure it's not inlined.
    pub fn submit(&'static self) {
        use Ordering::{Acquire, Relaxed, Release};

        let node = &self.node;

        if let Err(mut state) = node
            .state
            .compare_exchange(PENDING, RUNNING, Relaxed, Acquire)
        {
            while state != COMPLETED {
                core::hint::spin_loop();
                state = node.state.load(Acquire);
            }

            return;
        }

        let reg = <T as Collect>::registry();

        debug_assert_eq!(
            reg.type_id,
            TypeId::of::<T>(),
            "\n\
            ════════════════════════════════════════════════════════════════\n\
                Type Safety Violation in Zlim Registry                 \n\
            ════════════════════════════════════════════════════════════════\n\
                Operation: submit\n\
                Note: The submitted data type does not match the registry.\n\
                Registry type: `?`(TypeId: {:?})\n\
                Found type:    `{}`(TypeId: {:?})\n\
            ════════════════════════════════════════════════════════════════\n\
            ",
            reg.type_id,
            core::any::type_name::<T>(),
            TypeId::of::<T>(),
        );

        let mut head = reg.head.load(Relaxed);

        loop {
            unsafe {
                *node.next.get() = head.as_ref();
            }

            let new_head = node as *const Node as *mut Node;

            if let Err(prev) = reg.head.compare_exchange(head, new_head, Release, Relaxed) {
                head = prev;
                continue;
            }

            node.state.store(COMPLETED, Release);
            return;
        }
    }
}

unsafe impl<T: Collect> Sync for Iter<T> {}
unsafe impl<T: Collect> Send for Iter<T> {}

impl<T: Collect> Copy for Iter<T> {}
impl<T: Collect> Clone for Iter<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Collect> FusedIterator for Iter<T> {}

impl<T: Collect> Iterator for Iter<T> {
    type Item = &'static T;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.node?;

        let ptr = node.data as *const T;
        debug_assert!(ptr.is_aligned());

        self.node = unsafe { *node.next.get() };
        unsafe { Some(&*ptr) }
    }
}

// ----------------------------------------------------------------------------
// iter

/// Returns an iterator over all submitted `T` values.
///
/// The iteration order is unspecified.
///
/// # Example
///
/// ```no_run
/// struct Flag(u8);
///
/// zlim_reg::collect!(Flag);
/// zlim_reg::submit!(Flag(1) => Flag);
///
/// let _ = zlim_reg::iter::<Flag>().count();
/// ```
pub fn iter<T: Collect>() -> Iter<T> {
    #[cfg(target_family = "wasm")]
    call_ctor_in_wasm();

    let reg = <T as Collect>::registry();

    assert_eq!(
        reg.type_id,
        TypeId::of::<T>(),
        "\n\
        ════════════════════════════════════════════════════════════════\n\
            Type Safety Violation in Zlim Registry                 \n\
        ════════════════════════════════════════════════════════════════\n\
            Operation: iter\n\
            Note: The same Registry may be reused for different types.\n\
            Registry type:  `?`(TypeId: {:?})\n\
            Iter Item type: `{}`(TypeId: {:?})\n\
        ════════════════════════════════════════════════════════════════\n\
        ",
        reg.type_id,
        core::any::type_name::<T>(),
        TypeId::of::<T>(),
    );

    let head = reg.head.load(Ordering::Acquire);
    unsafe {
        Iter {
            node: head.as_ref(),
            _marker: PhantomData,
        }
    }
}

#[cfg(target_family = "wasm")]
fn call_ctor_in_wasm() {
    unsafe extern "C" {
        unsafe fn __wasm_call_ctors();
    }

    static ONCE_FLAG: AtomicUsize = AtomicUsize::new(PENDING);

    // `__wasm_call_ctors` is idempotent and only needs to be called once;
    // a simple atomic flag prevents re-entry.  There is no running state.
    if ONCE_FLAG.load(Ordering::Acquire) != COMPLETED {
        // SAFETY:
        // `__wasm_call_ctors()` is the standard WASM constructor
        // initializer (emitted by the linker when `.init_array` entries
        // exist).  Subsequent calls after the first are harmless
        // because the constructors themselves are idempotent, but we
        // still guard with an atomic flag to avoid unnecessary work.
        unsafe {
            __wasm_call_ctors();
        }
        ONCE_FLAG.store(COMPLETED, Ordering::Release);
    }
}

// ----------------------------------------------------------------------------
// macros

/// Associates an registry with the specified type.
///
/// This macro must be invoked in the same crate that defines the type.
///
/// # Example
///
/// ```no_run
/// struct Flag;
///
/// zlim_reg::collect!(Flag);
/// ```
#[macro_export]
macro_rules! collect {
    ($ty:ty) => {
        impl $crate::Collect for $ty {
            #[inline]
            fn registry() -> &'static $crate::Registry {
                static REGISTRY: $crate::Registry = $crate::Registry::new::<$ty>();
                &REGISTRY
            }
        }
    };
}

/// Submits a value to the registry of a given type.
///
/// Supported forms:
/// - `submit!(value => Type)` creates a private static item.
/// - `submit!(value => Type as NAME)` creates a public named static [`Item`].
///
/// This macro is intended for module scope (outside function bodies).
///
/// # Example
///
/// ```no_run
/// struct Flag(u8);
///
/// zlim_reg::collect!(Flag);
///
/// zlim_reg::submit!(Flag(1) => Flag);
/// zlim_reg::submit!(Flag(2) => Flag as FLAG_TWO);
///
/// let _ = zlim_reg::iter::<Flag>().count();
/// ```
#[macro_export]
macro_rules! submit {
    ($value:expr => $ty:ty as $ident:ident) => {
        pub static $ident: $crate::Item<$ty> = {
            static __VALUE__: $ty = $value;
            <$crate::Item<$ty>>::new(&__VALUE__)
        };

        const _: () = {
            $crate::__call_ctor!($ident, $ty);
        };
    };
    ($value:expr => $ty:ty) => {
        const _: () = {
            static __ITEM__: $crate::Item<$ty> = {
                static __VALUE__: $ty = $value;
                <$crate::Item<$ty>>::new(&__VALUE__)
            };

            $crate::__call_ctor!(__ITEM__, $ty);
        };
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __call_ctor {
    ($ident:ident, $ty:ty) => {
        #[cfg_attr(
            any(target_os = "linux", target_os = "android"),
            unsafe(link_section = ".text.startup")
        )]
        unsafe extern "C" fn __ctor() {
            <$crate::Item<$ty>>::submit(&$ident);
        }

        // Linux/ELF:
        // - https://refspecs.linuxfoundation.org/LSB_5.0.0/LSB-Core-generic/LSB-Core-generic/specialsections.html
        // - https://www.exploit-db.com/papers/13234
        //
        // macOS:
        // - https://developer.apple.com/library/archive/documentation/DeveloperTools/Reference/Assembler/040-Assembler_Directives/asm_directives.html
        // - https://stackoverflow.com/questions/59271654/what-does-attribute-constructor-compile-to-for-ios-how-to-do-it-in-hand
        //
        // Windows:
        // - https://learn.microsoft.com/en-us/cpp/c-runtime-library/crt-initialization
        // - https://www.cnblogs.com/sunkang/archive/2011/05/24/2055635.html
        // - What is `.CRT`?: `CRT`=C Runtime Library
        // - What is `$XCU`?: `X`=Fixed-Prefix, `C`=C++ init, `U`=User
        #[used]
        #[cfg_attr(windows, unsafe(link_section = ".CRT$XCU"))]
        #[cfg_attr(
            any(target_os = "macos", target_os = "ios", target_os = "tvos",),
            unsafe(link_section = "__DATA,__mod_init_func,mod_init_funcs")
        )]
        #[cfg_attr(
            any(
                target_family = "wasm",
                target_os = "linux",
                target_os = "android",
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "fuchsia",
                target_os = "illumos",
                target_os = "netbsd",
                target_os = "openbsd",
                target_os = "redox",
                target_os = "solaris",
                target_os = "haiku",
                target_os = "vxworks",
                target_os = "nto",
                target_os = "none",
            ),
            unsafe(link_section = ".init_array")
        )]
        static __CTOR: unsafe extern "C" fn() = __ctor;
    };
}
