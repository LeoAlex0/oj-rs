use std::{
    collections::{HashMap, HashSet},
    ops::{Add, Sub},
};

pub trait ID: Eq + Copy {}
pub trait VertexID: ID {}
pub trait EdgeID<V: VertexID>: ID {
    fn from(&self) -> V;
    fn to(&self) -> V;
}

pub trait LCTBasic<V: VertexID, E: EdgeID<V>> {
    fn link(&self, v: V, w: V) -> Self;
    fn cut(&self, v: V, w: V) -> Self;
    fn evert(&self, v: V) -> Self;
}

pub trait LCTValue<V: VertexID, E: EdgeID<V>, W: Ord + Add<W> + Sub<W>>: LCTBasic<V, E> {
    fn parent(&self, v: V) -> Option<V>;
    fn root(&self, v: V) -> V;
    fn cost(&self, v: V) -> W;
    fn mincost(&self, v: V) -> W;
    fn update(&self, v: V, x: W) -> Self;
}

pub struct TrivialLCTImpl<V: VertexID, E: EdgeID<V>, W: Ord + Add<W> + Sub<W>> {
    pub vertex: HashSet<V>,
    pub edges: HashMap<E, W>,
}
