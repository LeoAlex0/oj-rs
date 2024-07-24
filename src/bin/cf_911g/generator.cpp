#include <iostream>

using namespace std;

constexpr int N = 200000;
constexpr int M = 200000;

int main()
{
    ios::sync_with_stdio(0);
    cin.tie(0);
    cout << N << "\n";
    for (int i = 0; i < N; i++)
    {
        cout << (i % 100) + 1;
        if (i < N - 1)
            cout << " ";
        else
            cout << "\n";
    }
    cout << M << "\n";
    for (int i = 0; i < M; i++)
    {
        int l = 1 + std::rand() % N;
        int r = 1 + std::rand() % N;
        cout << std::min(l, r) << " " << std::max(l, r) << " " << 1 + i % 100 << " " << 1 + (i + 1) % 100;
        if (i < M - 1)
            cout << "\n";
        else
            cout << endl;
    }
    return 0;
}
