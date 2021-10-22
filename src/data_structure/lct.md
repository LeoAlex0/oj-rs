# LCT 论文笔记

> 论文原文见[此处](https://www.cs.cmu.edu/~sleator/papers/dynamic-trees.pdf)

## 概述

结构维护了一个顶点不共有的树的集合

结构包括如下几个基础操作

* `link(v,w)`：如果`v`是某一棵树的根，并且`w`是另一颗树上的节点，那么连结两点，使得两棵树合并。
* `cut(v)`：如果`v`不是任何一棵树的根，那么删除v和其父节点之间的连结，使得其所在的树分解为两棵。
* `evert(v)`：将`v`所在的树进行翻转，使得`v`变为根。

论文中给出了2种实现，其中：

* 第一种在最坏情况的操作序列下能保证各操作**平均**时间复杂度为$O(\log n)$
* 第二种实现略微复杂，但能保证各操作**最坏**时间复杂度上界为$O(\log n)$

## 问题

考虑需要考虑如下操作的版本的一个动态树问题：

* `parent(v: vertex)`: 返回`v`的父节点，如果`v`是根，则返回一个特殊值`null`
* `root(v: vertex)`: 返回`v`所在的树的根节点
* `cost(v: vertex)`: 返回`edge(v, parent(v))`上的边权，此操作假定v非树上的根节点。
* `mincost(v: vertex)`: 返回从`v`到`root(v)`路径上`cost(i)`最小的节点`i`，若有多个最小的则返回离`root(i)`最近的一个。此操作假定`v`非根节点。
* `update(v: vertex,x: num)`修改`v`到`root(v)`上所有边的`cost`，给它们加上`x`
* `link(v,w: vertex,x: num)`类似于上面，将`edge(v,w)`的`cost`设置为`x`
* `cut(v: vertex)`同上
* `evert(v: vertex)`同上

注意到：

* `parent,root,cost,mincost`操作是只读的
* `update`操作不改变森林的形态
* `link,cut,evert`改变树的形态

另外若我们舍弃`link`和`evert`中的一个，我们可以把`num`泛化成任意的半群。

我们假定森林的初始状态是数个离散的节点构成单个节点的树（没有边相连）

考察一种显而易见的写法：

* 对每个节点`v`，我们保存其父节点`p(v)`和其边`edge(v,p(v))`上的`cost`

使用这种表示，我们可以做到：

* $O(1)$时间的`parent,cost,link,cut`
* 最坏$O(n)$时间的`mincost,update,evert,root`

> 证明显而易见，在树退化为链且每次对叶子操作时是最坏情况。

## 解

首先假定我们知道如何解决一个特殊版本的动态树问题。
这个特殊情况假定我们知道下面11个基础操作的动态“树”问题：

* `path(v: vertex)`: 返回一个包含`v`的路径
  * 假定路径是一个唯一的标识符
* `head(p: path)`: 返回路径`p`的头（第一个节点）
* `tail(p: path)`: 返回路径`p`的尾（最后一个节点）
* `before(v: vertex)`: 返回路径`path(v)`上`v`的前一个节点，若`v`是头，则返回`null`
* `after(v: vertex)`: 返回路径`path(v)`上`v`的后一个节点，若`v`为尾，则返回`null`
* `pcost(v: vertex)`: 返回`edge(v,after(v))`的`cost`。这个操作假定`v`非路径尾。
* `pmincost(p: path)`: 返回路径`p`中`edge(v,after(v))`上`cost`最小的`v`，若有多个，则返回最接近`tail(v)`的
* `pupdate(p: path,x: num)`: 对路径`p`上每条边的`cost`加上`x`
* `reverse(p: path)`: 将路径`p`反转，尾作头而头作尾
* `concatenate(p,q: path,x: real)`: 添加一条边`edge(tail(p),head(q))`连结两条路径，并设置该边的`cost`为`x`，返回新的path
* `split(v: vertex)`: 拆分`path(v)`为三个部分，返回四元组`(p,q: path,x,y: num)`,其中：
  * `p`: `head(path(v)`到`before(v)`的子路径
  * `q`: `after(v)`到`tail(path(v))`的子路径
  * `x`: `edge(before(v),v)`的`cost`
  * `y`: `edge(v,after(v))`的`cost`
  * 若无良定义，则返回`null`

使用上述基础操作，通过将每棵树划分成不相交的路径的集合，我们便可以解决动态树问题。

针对上述划分，我们有两种方法：

* 显然的划分法: 对每次树操作平均$O(\log n)$的时间
* 基于大小的划分法: 对每次树操作最坏$O(\log n)$的时间

### 显然的划分法

在这种划分方法中，树的划分结果与树的结构无关，而只与输入的操作序列有关。

我们将所有树中的边分为两类，实的（solid）和虚的（dashed），并且维护一条性质：

* 每个顶点最多只有**一条**实边连入（向根连结为正向）

因此实边维护了一组用于划分节点的实路径。
