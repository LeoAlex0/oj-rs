## 概述

给定一棵以$1$为根的包含 $n \in [1,10^5]$ 个节点的有根树。第$i$个节点有点权$a_i \in [1,10^5]$。

需要依次处理$q \in [1,10^5]$次操作：

1. `1 u`：对点`u`的点权$a_u$取异或$1$，即：$a_u'=a_u \oplus 1$
2. `2 u v`: 求出包含点$u$,$v$的所有路径中，对应点权的中位数最大的那一条路径的点权的*中位数*。

> 题目中的*中位数*意为：长度为$t$的序列中第$\lceil \frac {t+1} {2} \rceil$小的数，
> 即后中位数。

输入保证对操作2的每次询问：点$u$不是点$v$的祖先，且点$v$不是点$u$的祖先，且$v\ne u$。

## 方案

若构造一个判定函数用以比较大小：

$$
cmp(a,b) = \begin{cases}
    1 & a < b \\
    -1 & b \le a
\end{cases}
$$

可知，一个序列$a$的后中位数$m$可表示为：

$$
\begin{aligned}
m & \equiv \argmax_x \left\{ Card(\{ a_i < x\}) - Card(\{ x \le a_i \})  \le 0 \right\} \\
& = \argmax_x \left\{ \sum_i cmp(a_i,x) \le 0 \right\}
\end{aligned}
$$

其中$Card(x)$指集合$x$中元素的个数。

注意到对于操作2,点$u$和点$v$不在同一枝上，故对于目标路径$u' \to v'$，对称地，我们不妨设：

* $u' \in sub(u)$
* $v' \in sub(v)$

其中$sub(x)$意为以点$x$所在节点为根的子树中所有节点所在的集合。

由于我们需要让所求的后中位数最大，故我们的最终目标为：

$$
\begin{aligned}
ans &= \max_{u'\in sub(u),v'\in sub(v)} \left\{
    \argmax_{x} \left\{
        \sum_{i\in [u',v']} cmp(a_i,x) \le 0
        \right\}
    \right\} \\
    &= \max_{u'\in sub(u),v'\in sub(v)} \left\{
    \argmax_{x} \left\{
        \sum_{i\in [u',u)} cmp(a_i,x) +
        \sum_{i\in [u,v]} cmp(a_i,x) +
        \sum_{i\in (v,v']} cmp(a_i,x)
        \le 0
        \right\}
    \right\}
\end{aligned}
$$

其中：

* $[a,b]$ 为点 $a$ 到 $b$ 的路径上所有点构成的集合（ 包含点 $a,b$ ）
* $(a,b)$ 为点 $a$ 到 $b$ 的路径中所有点构成的集合（ 不含点 $a,b$ ）
* $[a,b)$ 与 $(a,b]$ 同理

注意到对于 $x$ 而言:

* $Card(\{ a_i < x\}) - Card(\{ x \le a_i \})=\sum_i cmp(a_i,x)$
对于$x$是单调递增的
* $\sum_{i\in (u,v)} cmp(a_i,x)$又是一常数（对于固定$x$）。

故若要最大化中位数，需要最小化剩余两项，即：

$$
\begin{aligned}
ans &= \max_{u'\in sub(u),v'\in sub(v)} \left\{
    \argmax_{x} \left\{
        \sum_{i\in [u',u)} cmp(a_i,x) +
        \sum_{i\in [u,v]} cmp(a_i,x) +
        \sum_{i\in (v,v']} cmp(a_i,x)
        \le 0
        \right\}
    \right\} \\
    &= \argmax_{x} \left\{
        \min_{u'\in sub(u)} \left\{
            \sum_{i\in [u',u)} cmp(a_i,x)
        \right\} +
        \sum_{i\in [u,v]} cmp(a_i,x) +
        \min_{v'\in sub(v)} \left\{
            \sum_{i\in (v,v']} cmp(a_i,x)
        \right\}
        \le 0
    \right\}
\end{aligned}
$$

所以我们需要一个结构用以维护：

$$
f(x,v) = \min_{v'\in sub(v)} \left\{
            \sum_{i\in [v',v)} cmp(a_i,x),0
        \right\}
$$

进而此时每次查询的答案为：

$$
\begin{aligned}
ans &= \argmax_{x} \left\{
        \min_{u'\in sub(u)} \left\{
            \sum_{i\in [u',u)} cmp(a_i,x)
        \right\} +
        \sum_{i\in (u,v)} cmp(a_i,x) +
        \min_{v'\in sub(v)} \left\{
            \sum_{i\in (v,v']} cmp(a_i,x)
        \right\}
        \le 0
    \right\} \\
    &= \argmax_{x} \left\{
        f(x,u) +
        \sum_{i\in [u,v]} cmp(a_i,x) +
        f(x,v)
        \le 0
    \right\}
\end{aligned}
$$

考虑到在题设与$f(x,a)$的表达式中:

1. 所有的修改只会导致$a_v = a_v \pm 1$，故只有$x=a_v,a_v\pm 1$两点需要修改，这是好的。
2. 对于所有$i\in sub(a)$，其对应的所有DFS遍历的*序号*是连续的，这是更好的。
   * *序号*指第几个被DFS遍历的
   * 对于多叉树可在访问任意两子结点之间/前/后访问根，该性质依然成立
3. $\sum_{i \in (v,v']} cmp(a_i,x) = \sum_{i \in [1,v']} cmp(a_i,x) - \sum_{i \in [1,v]} cmp(a_i,x)$，
这是坠吼滴。

但是注意到$f(x,v)$的表达式与$sub(v)$相关，这使得对其的维护与题设给出的树的形态有关，这是野蛮的。
> 一个直接的想法是将$f(x,v)$转化为在题设给的树上的递推式：
> $$
> \begin{aligned}
> f(x,v) &= \min_{v'\in sub(v)} \left\{
> \sum_{i\in [v',v)} cmp(a_i,x),0
> \right\} \\
> &= \min_{v'\in child(v)} \left\{
> f(x,v')+cmp(a_{v'},x),0
> \right\}
> \end{aligned}
> $$
> 其中： $child(v)$指点$v$所有子结点所在的集合
>
> 若如此维护，在更新时至少会替换一整条链。若题设给出的树退化成链，则最坏情况下需要反复重建，
> 复杂度无法接受。

### 查询操作的处理（静态部分）

考虑到性质2,3，我们可以准备一个结构，使用DFS遍历的序号来表示节点编号，进而可以将对$f(x,v)$
的求值转换为一个区间查询问题。

$$
\begin{aligned}
p(x,v) &\equiv  \sum_{i\in [1,ord^{-1}(v)]} cmp(a_i,x) \\
min_p (x,[L,R]) &= \min_{i\in [L,R]} \{ p(x,i) \} \\
f(x,v) &= \min_{v'\in sub(v)} \left\{
        \sum_{i\in [v',v)} cmp(a_i,x),0
    \right\} \\
    &= \min_{v'\in sub(v)} \left\{
        \sum_{i\in [1,v']} cmp(a_i,x) - \sum_{i \in [1,v]} cmp(a_i,x)
    \right\} & 性质3 \\
    &= \min_{v'\in sub(v)} \left\{
        \sum_{i \in [1,v']} cmp(a_i,x)
    \right\} - \sum_{i \in [1,v]} cmp(a_i,x) \\
    &= \min_{v' \in [L_m(ord(v)),R_m(ord(v))]} \left\{
        \sum_{i \in [1,ord^{-1}(v')]} cmp(a_i,x)
    \right\} - \sum_{i \in [1,ord^{-1}(ord(v))]} cmp(a_i,x) \\
    &= \min_{v' \in [L_m(ord(v)),R_m(ord(v))]} \{ p(x,v') \} - p(x,ord(v)) \\
    &= min_p(x,[L_m(ord(v)),R_m(ord(v))]) - p(x,ord(v))
\end{aligned}
$$

其中:

* $L_m(v)$指DFS序编号为$v$的节点所在子树上最小的DFS序编号
* $R_m(v)$指DFS序编号为$v$的节点所在子树上最大的DFS序编号
* $ord(v)$指节点$v$的DFS序编号
  * 显然一个节点和它的DFS序编号一一对应，故它的逆$ord^{-1}(v)$可被定义

故我们可以将对$f(x,v)$的求值转换为对$p(x)$的区间最小值查询与单点查询。

但是注意到$p(x,v)$的求值过程依然与题设给出的树结构相关，
不过我们可以利用$p(x,v)$在$x$上的递推关系最小化题设的树结构给出的影响。

$$
\begin{aligned}
p(x,v) &= \sum_{i\in [1,ord^{-1}(v)]} cmp(a_i,x) \\
&= \begin{cases}
    -1 &  x = 0 \wedge \underbrace{ord^{-1}(v)= 1}_{根节点}  \\
    p(0,\underbrace{ord(parent(ord^{-1}(v)))}_{父节点})-1 & x = 0 \wedge ord^{-1}(v)\ne 1 \\
    p(x-1,v) + 2 \times \underbrace{count(\overbrace{[1,ord^{-1}(v)]}^{到v的路径},x-1)}_{到v的路径上点权为x-1的节点个数} & otherwise
\end{cases}
\end{aligned}
$$

注意这个$count$，可以发现，如果我们对于$p(x-1,*)$所在的结构进行区间修改，
得到的新结构即可满足$p(x,*)$的查询。
> 具体的方法是: 对于$p(x-1,*)$中的所有满足$a_{ord^{-1}(v)}=x-1$的$v$，将$i\in [L_m(v),R_m(v)]$处的$p(x-1,i)+=2$，最终得到的结构即可用于维护$p(x,*)$

至此，我们可以给维护$p(x,*)$的结构所要求的操作做个总结：

* $p(x,*)$区间修改：$(+2)$ （考虑到修改应该是$(+k)$）
* $p(x,*)$区间查询：$\min$
* $p(x,*)$单点查询（当然也可以理解为单点的$\min$）

可以发现，$(\mathbb{R},\min)$构成一个幺半群，而$+k$与函数复合$.$构成可交换幺半群，并且是$\mathbb{R}$上的一个*算子幺半群(Operator Monoid)*，且在$\min$上有分配律，故可以考虑线段树实现。
> 即满足以下性质：
>
> * $\min$的结合律：$\forall a,b,c: \min(a,\min(b,c)) = \min(\min(a,b),c)$ (显然)
> * $\min$的单位元：$\exist e: \forall a: \min(e,a) = \min(a,e) = a$
>   * 对本题设，任取$e>10^5$，均满足条件
>   * 当然不失一般地，也可以通过拓展一个$+\infty$以满足条件
> * $(+k)$对$\min$分配律：$\forall a,b,k: (+k)(\min(a,b)) = \min((+k)(a),(+k)(b))$
>   * 其中：定义函数$(+k)$为$(+k)(m) = m+k$
>   * 故，$左=\min(a,b)+k=\min(a+k,b+k)=右$，得证
> * $(+k)$在$\mathbb{R}$上的算子：$\forall k_1,k_2,a:((+k_1).(+k_2))(a) = (+k_1)((+k_2)(a))$
> * $(+k)$的结合律：
> $\forall k_1,k_2,k_3: ((+k_1).(+k_2)).(+k_3) = (+k_1).((+k_2).(+k_3))$
>   * 其中：定义$(f.g)(x) = f(g(x))$
>   * 故，$\forall x:左(x)=((x+k_3)+k_2)+k_1=(x+(k_3+k_2))+k_1=右(x)$,得证
> * $(+k)$的单位元：$\exists e:\forall k:(+k).(+e)=(+e).(+k)=(+k)$
>   * 取$e=0$即可
> * $(+k)$的交换律: $\forall k_1,k_2: (+k_1).(+k_2) = (+k_2).(+k_1)$
>

在考虑完上述问题之后，再看我们所需要的查询结果$ans$，我们可以看到：
$$
\begin{aligned}
ans &= \argmax_{x} \left\{
        f(x,u) +
        \sum_{i\in [u,v]} cmp(a_i,x) +
        f(x,v)
        \le 0
    \right\} \\
    &= \argmax_{x} \left\{
        min_p(x,[L_m(ord(u)),R_m(ord(u))]) - p(x,ord(u)) +
        \sum_{i\in [u,v]} cmp(a_i,x) +
        min_p(x,[L_m(ord(v)),R_m(ord(v))]) - p(x,ord(v))
        \le 0
    \right\} \\
    &= \argmax_{x} \left\{
        min_p(x,[L_m(ord(u)),R_m(ord(u))]) - p(x,ord(u)) +
        p(x,ord(u)) - p(x,ord(lca(u,v))) +
        p(x,ord(v)) - p(x,ord(lca(u,v))) +
        cmp(a_{lca(u,v)},x) +
        min_p(x,[L_m(ord(v)),R_m(ord(v))]) - p(x,ord(v))
        \le 0
    \right\} \\
    &= \argmax_{x} \left\{
        min_p(x,[L_m(ord(u)),R_m(ord(u))]) +
        min_p(x,[L_m(ord(v)),R_m(ord(v))])
        - 2p(x,ord(lca(u,v))) +
        cmp(a_{lca(u,v)},x)
        \le 0
    \right\}
\end{aligned}
$$

其中：$lca(u,v)$指在题设给出的树中，点$u,v$的最近公共祖先。

不过对于$lca$的查找严重依赖题目所给的树的形态，故此处需要*树链剖分*来保证最坏情况复杂度不超标。
> 一个可能的替代是离线化后使用`tarjan`算法一次性扫出所有需要的$lca$。
>
> 但是：
>
> 1. 我不喜欢离线化，因为离线化会限制算法的实际使用场景
> 2. 既然要离线化，为何不干脆考虑些更为激进的写法？

考虑到单调性，我们可以直接在此基础上二分查找$ans$。

故静态地，复杂度为：

* 预处理时间：最坏$O(\max(m,n)\lg n)$
* 预处理空间：最坏$O(\max(m,n)+m\lg n)$
* 每次查询：最坏$O(\lg m\lg n)$

其中$m$为节点权值$a_i$的最大值。

### 写入操作的处理（动态部分）

直接考虑更一般地，若对于某一个节点$v$，其点权由$a_v$变成了$a_v'$，则有：

$$
\begin{aligned}
    cmp(a_v',x) &= cmp(a_v,x) + \begin{cases}
        0 & x \le \min(a_v,a_v') \vee \max(a_v,a_v') < x \\
        2 & \min(a_v,a_v') < x \le \max(a_v,a_v') \wedge a_v' \le a_v \\
        -2 & \min(a_v,a_v') < x \le \max(a_v,a_v') \wedge a_v \le a_v' \\
    \end{cases}
\end{aligned}
$$

显然，只有$x \in (\min(a_v,a_v'),\max(a_v,a_v']$的$cmp$发生了变化，
故只需要考虑该区间内$p(x,*)$的更新即可。

> 不过出于本题只会导致$a_v' = a_v \pm 1$，所以无需考虑使用支持该区间操作的数据结构。

进一步地，若$a_v$的$cmp$发生了变化，那么对于$p(x,*)$而言，位于$[L_m(ord(v)),R_m(ord(v))]$的节点均需要加上:

$$
\begin{aligned}
    \Delta_{cmp} &= \begin{cases}
        0 & x \le \min(a_v,a_v') \vee \max(a_v,a_v') < x \\
        2 & \min(a_v,a_v') < x \le \max(a_v,a_v') \wedge a_v' \le a_v \\
        -2 & \min(a_v,a_v') < x \le \max(a_v,a_v') \wedge a_v \le a_v' \\
    \end{cases} \\
    p'(x,u) &= p(x,u) + \begin{cases}
        \Delta_{cmp} & u \in [L_m(ord(v)),R_m(ord(v))] \\
        0 & otherwise
    \end{cases}
\end{aligned}
$$
