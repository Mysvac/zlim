//! Extra collection-like utilities.

mod cache_paded;
mod parallel;

pub mod array_deque;
pub mod block_list;
pub mod thread_local;
pub mod type_map;

pub use array_deque::ArrayDeque;
pub use block_list::BlockList;
pub use cache_paded::CachePadded;
pub use parallel::Parallel;
pub use thread_local::ThreadLocal;
pub use type_map::TypeMap;
