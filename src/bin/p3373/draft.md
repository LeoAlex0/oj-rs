# P3373

$$
a_i' = a_i * x\\
a_i' = a_i + x\\
f_i(x) = k_i*x + b_i \\

\begin{aligned}
(f_1 \circ f_2)(x) &= k_1(k_2x+b_2)+b_1 \\
&= k_1k_2x + (k_1b_2+b_1)\\
\sum_{i\in[l,r]} f_m(a_i) &= \sum_{i\in[l,r]} k_ma_i + b_m \\
&= k_m \sum_{i\in[l,r]} a_i + \sum_{i\in[l,r]} b_m \\
&= k_m \sum_{i\in[l,r]} a_i + Card([l,r])b_m
\end{aligned}
$$
