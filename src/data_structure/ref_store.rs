use std::{cell::Cell, marker::PhantomData, mem::MaybeUninit, rc::Rc, sync::Arc};

pub trait RefStore<T> {
    type Ref: Clone;

    fn alloc(&mut self, value: T) -> Self::Ref;
    fn with_ref<U, F>(&self, reference: &Self::Ref, f: F) -> U
    where
        F: FnOnce(&T) -> U;
}

pub trait RefStoreMut<T>: RefStore<T> {
    fn set_ref(&mut self, reference: &Self::Ref, value: T);
}

pub trait RefStoreFactory {
    type Store<T>;

    fn store<T>(&self) -> Self::Store<T>;
}

pub trait RefMapper<T> {
    type Source;

    fn map_ref(value: &Self::Source) -> T;
}

pub struct LayeredStore<'base, Base, Scratch, Mapper> {
    base: &'base Base,
    scratch: Scratch,
    mapper: PhantomData<fn() -> Mapper>,
}

pub struct LayeredArenaStoreFactory<'base, 'scratch, Base, Mapper> {
    base: &'base Base,
    capacity: usize,
    marker: PhantomData<fn() -> (&'scratch (), Mapper)>,
}

// `LayeredRef` 把“这个引用来自哪里”显式编码进引用值里。这样不同 arena/store
// 可以安全地叠在一起：旧引用继续读 base，新分配只进入 scratch。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayeredRef<BaseRef, ScratchRef> {
    Base(BaseRef),
    Scratch(ScratchRef),
}

#[derive(Default)]
pub struct RcStore;

#[derive(Default)]
pub struct ArcStore;

#[derive(Default)]
pub struct RcStoreFactory;

#[derive(Default)]
pub struct ArcStoreFactory;

impl<T> RefStore<T> for RcStore {
    type Ref = Rc<T>;

    fn alloc(&mut self, value: T) -> Self::Ref {
        Rc::new(value)
    }

    fn with_ref<U, F>(&self, reference: &Self::Ref, f: F) -> U
    where
        F: FnOnce(&T) -> U,
    {
        f(reference.as_ref())
    }
}

impl<T> RefStore<T> for ArcStore {
    type Ref = Arc<T>;

    fn alloc(&mut self, value: T) -> Self::Ref {
        Arc::new(value)
    }

    fn with_ref<U, F>(&self, reference: &Self::Ref, f: F) -> U
    where
        F: FnOnce(&T) -> U,
    {
        f(reference.as_ref())
    }
}

impl RefStoreFactory for RcStoreFactory {
    type Store<T> = RcStore;

    fn store<T>(&self) -> Self::Store<T> {
        RcStore
    }
}

impl RefStoreFactory for ArcStoreFactory {
    type Store<T> = ArcStore;

    fn store<T>(&self) -> Self::Store<T> {
        ArcStore
    }
}

impl<'base, Base, Scratch, Mapper> LayeredStore<'base, Base, Scratch, Mapper> {
    #[inline]
    pub fn new(base: &'base Base, scratch: Scratch) -> Self {
        Self {
            base,
            scratch,
            mapper: PhantomData,
        }
    }

    #[inline]
    pub fn with_base_ref<T, U, F>(&self, reference: &<Base as RefStore<T>>::Ref, f: F) -> U
    where
        Base: RefStore<T>,
        F: FnOnce(&T) -> U,
    {
        self.base.with_ref(reference, f)
    }

    #[inline]
    pub fn with_scratch_ref<T, U, F>(&self, reference: &<Scratch as RefStore<T>>::Ref, f: F) -> U
    where
        Scratch: RefStore<T>,
        F: FnOnce(&T) -> U,
    {
        self.scratch.with_ref(reference, f)
    }
}

impl<'base, 'scratch, Base, Mapper> LayeredArenaStoreFactory<'base, 'scratch, Base, Mapper> {
    #[inline]
    pub fn new(base: &'base Base, capacity: usize) -> Self {
        Self {
            base,
            capacity,
            marker: PhantomData,
        }
    }

    #[inline]
    pub fn store_with_capacity<T>(
        &self,
        capacity: usize,
    ) -> LayeredStore<'base, Base, Arena<'scratch, T>, Mapper> {
        LayeredStore::new(self.base, Arena::with_capacity(capacity))
    }
}

impl<'base, 'scratch, Base, Mapper> RefStoreFactory
    for LayeredArenaStoreFactory<'base, 'scratch, Base, Mapper>
{
    type Store<T> = LayeredStore<'base, Base, Arena<'scratch, T>, Mapper>;

    fn store<T>(&self) -> Self::Store<T> {
        self.store_with_capacity(self.capacity)
    }
}

impl<T, Base, Scratch, Mapper> RefStore<T> for LayeredStore<'_, Base, Scratch, Mapper>
where
    Base: RefStore<Mapper::Source>,
    Scratch: RefStore<T>,
    Mapper: RefMapper<T>,
{
    type Ref = LayeredRef<<Base as RefStore<Mapper::Source>>::Ref, <Scratch as RefStore<T>>::Ref>;

    #[inline]
    fn alloc(&mut self, value: T) -> Self::Ref {
        LayeredRef::Scratch(self.scratch.alloc(value))
    }

    #[inline]
    fn with_ref<U, F>(&self, reference: &Self::Ref, f: F) -> U
    where
        F: FnOnce(&T) -> U,
    {
        match reference {
            LayeredRef::Base(reference) => self.base.with_ref(reference, |value| {
                let mapped = Mapper::map_ref(value);
                f(&mapped)
            }),
            LayeredRef::Scratch(reference) => self.scratch.with_ref(reference, f),
        }
    }
}

type Brand<'id> = PhantomData<Cell<&'id ()>>;

pub struct Arena<'id, T> {
    storage: Vec<T>,
    brand: Brand<'id>,
}

pub struct ConstArena<'id, T, const N: usize> {
    storage: Box<[MaybeUninit<T>; N]>,
    len: usize,
    brand: Brand<'id>,
}

pub struct ArenaStoreFactory<'id> {
    capacity: usize,
    brand: Brand<'id>,
}

pub struct ConstArenaStoreFactory<'id, const N: usize> {
    brand: Brand<'id>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ArenaRef<'id> {
    index: usize,
    brand: Brand<'id>,
}

impl ArenaStoreFactory<'_> {
    pub fn scoped<T, F>(capacity: usize, f: F) -> T
    where
        F: for<'id> FnOnce(ArenaStoreFactory<'id>) -> T,
    {
        f(ArenaStoreFactory {
            capacity,
            brand: PhantomData,
        })
    }
}

impl<const N: usize> ConstArenaStoreFactory<'_, N> {
    pub fn scoped<T, F>(f: F) -> T
    where
        F: for<'id> FnOnce(ConstArenaStoreFactory<'id, N>) -> T,
    {
        f(ConstArenaStoreFactory { brand: PhantomData })
    }
}

impl<'id, T> Arena<'id, T> {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            storage: Vec::with_capacity(capacity),
            brand: PhantomData,
        }
    }
}

impl<'id, T, const N: usize> ConstArena<'id, T, N> {
    fn new(brand: Brand<'id>) -> Self {
        // SAFETY: 目标类型是 `[MaybeUninit<T>; N]`，未初始化的 `MaybeUninit<T>`
        // 本身就是合法值。这里直接在堆上得到固定长度数组，避免大 N 时压栈。
        let storage = unsafe { Box::new_uninit().assume_init() };
        Self {
            storage,
            len: 0,
            brand,
        }
    }
}

impl<'id> RefStoreFactory for ArenaStoreFactory<'id> {
    type Store<T> = Arena<'id, T>;

    fn store<T>(&self) -> Self::Store<T> {
        Arena {
            storage: Vec::with_capacity(self.capacity),
            brand: self.brand,
        }
    }
}

impl<'id, const N: usize> RefStoreFactory for ConstArenaStoreFactory<'id, N> {
    type Store<T> = ConstArena<'id, T, N>;

    fn store<T>(&self) -> Self::Store<T> {
        ConstArena::new(self.brand)
    }
}

impl<'id, T> RefStore<T> for Arena<'id, T> {
    type Ref = ArenaRef<'id>;

    #[inline]
    fn alloc(&mut self, value: T) -> Self::Ref {
        let index = self.storage.len();
        self.storage.push(value);
        ArenaRef {
            index,
            brand: self.brand,
        }
    }

    #[inline]
    fn with_ref<U, F>(&self, reference: &Self::Ref, f: F) -> U
    where
        F: FnOnce(&T) -> U,
    {
        f(&self.storage[reference.index])
    }
}

impl<'id, T> RefStoreMut<T> for Arena<'id, T> {
    fn set_ref(&mut self, reference: &Self::Ref, value: T) {
        self.storage[reference.index] = value;
    }
}

impl<'id, T, const N: usize> RefStore<T> for ConstArena<'id, T, N> {
    type Ref = ArenaRef<'id>;

    #[inline]
    fn alloc(&mut self, value: T) -> Self::Ref {
        assert!(self.len < N, "const arena capacity exceeded");
        let index = self.len;
        self.storage[index].write(value);
        self.len += 1;
        ArenaRef {
            index,
            brand: self.brand,
        }
    }

    #[inline]
    fn with_ref<U, F>(&self, reference: &Self::Ref, f: F) -> U
    where
        F: FnOnce(&T) -> U,
    {
        assert!(
            reference.index < self.len,
            "const arena reference points outside initialized storage"
        );
        // SAFETY: `alloc` 只会返回小于 `len` 的下标，且 `[0, len)` 内的槽位都已初始化。
        // `ArenaRef` 的 brand 防止不同 scoped arena 之间的引用混用。
        f(unsafe { self.storage[reference.index].assume_init_ref() })
    }
}

impl<'id, T, const N: usize> RefStoreMut<T> for ConstArena<'id, T, N> {
    fn set_ref(&mut self, reference: &Self::Ref, value: T) {
        assert!(
            reference.index < self.len,
            "const arena reference points outside initialized storage"
        );
        // SAFETY: 同上，目标槽位已经初始化；先析构旧值，再原址写入新值。
        unsafe {
            self.storage[reference.index].assume_init_drop();
        }
        self.storage[reference.index].write(value);
    }
}

impl<'id, T, const N: usize> Drop for ConstArena<'id, T, N> {
    fn drop(&mut self) {
        for slot in &mut self.storage[..self.len] {
            // SAFETY: `len` 之前的槽位正是 `alloc` 成功初始化过的槽位。
            unsafe {
                slot.assume_init_drop();
            }
        }
    }
}
