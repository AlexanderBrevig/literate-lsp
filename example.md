# Fibonacci in Forth

Let's implement the classic Fibonacci sequence. We'll start by defining a recursive
function that calculates the nth Fibonacci number.

The algorithm uses conditional logic to return 1 for n < 2, otherwise recursively computes fib(n-1) + fib(n-2).

```forth
: fib ( n -- fib ) dup 2 < if drop 1 else dup 1- fib swap 2 - fib + then ;
```

Now that we have our Fibonacci function defined, let's test it by computing the 5th Fibonacci number, which should give us 8.

```forth
5 fib .
```

Try doing a `hover` or a `goto definition` on the `fib` word!
