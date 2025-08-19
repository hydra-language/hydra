# Hydra

A statically-typed programming language because I got bored and liked Rust

***

## ✨ Features

* **Static Typing**: Strong type safety to catch errors at compile time.
* **Clear Syntax**: A clean and familiar C and Rust-style syntax that is easy to read and write.
* **Memory Safety**: Explicit control over mutability with `let` and `const` keywords.
* **Primitives**: A comprehensive set of integer, floating-point, character, and boolean types.
* **Powerful Arrays**: Built-in support for fixed-size arrays with control over element mutability
* **Control Flow**: Intuitive range-based `for` loops, `forEach` loops for collections, and standard `while` loops.

***

## 📜 Language Specification

### Primitives

The language provides a set of fundamental data types:

* **Signed Integers**: `i8`, `i16`, `i32`, `i64`
* **Unsigned Integers**: `u8`, `u16`, `u32`, `u64`
* **Floating-Point**: `f32`, `f64`
* **Character**: `char` (delimited by single quotes, e.g., `'a'`)
* **Boolean**: `bool` (`true` or `false`)
* **String**: `string` (a pseudo-primitive, delimited by double quotes, e.g., `"hello"`)

### Keywords

|           |           |           |           |                                            
| :-------- | :-------- | :-------- | :-------- |
| `let`     | `const`   | `struct`  | `fn`      |
| `return`  | `void`    | `in`      | `for`     |
| `if`      | `else`    | `forEach` | `while`   |
| `break`   | `skip`    | `include` | `typedef` |
| `None`    | `as`      | `size`    | `match`   |

* `skip`: Equivalent to `continue` in other languages.
* `include`: Used for importing modules.
* `typedef`: Used for creating type aliases.

***

## ⚙️ Syntax and Examples

### Variable Declaration

Variables are declared using `let` for mutable bindings and `const` for immutable bindings. Type annotations are mandatory.

```rust
// A mutable 32-bit integer
let x: i32 = 10;
x = 22; // This is valid

// An immutable 32-bit float
const PI: f32 = 3.14;
PI = 3.14159; // This would cause a compile-time error
```

### Arrays

Arrays have a fixed size and a declared element type. You can specify mutability for both the array binding and its elements.

**Declaration Syntax**: `<binding> <name>: [<mutability?> <type>, <size>] = { <elements> };`

```rust
// A mutable array of mutable i32s
let a: [i32, 3] = { 1, 2, 3 };
a[0] = 100; // Valid

// A mutable array of immutable chars
let b: [const char, 5] = { 'h', 'e', 'l', 'l', 'o' };
b[0] = 'j'; // Compile-time error: elements are const

// An immutable array of mutable i64s
const c: [i64, 3] = { 1, 2, 3 };
c = { 4, 5, 6 }; // Compile-time error: binding is const

// A fully immutable array
const d: [const char, 2] = { 'x', 'y' };
```

### Array Slicing

Slicing creates a view or a copy of a portion of an array.

* **Read-only Slice**: Use the `&` prefix to create a reference slice to a section of the original array.
* **Mutable Slice**: Use the `|array|` syntax to create a new, mutable slice on the heap.

```rust
let arr: [i32, 5] = {1, 2, 3, 4, 5};

// Create a read-only slice of the first three elements.
let read_only_slice: [i32, 3] = &arr[0..2];

// Create a new, mutable slice on the heap.
// NOTE: what allocates it on the heap is the bars between arr -> |arr|
let heap_slice: [i32, 2] := |arr|[3..5];
heap_slice[0] = 40; // This is valid
```

### Functions

Functions are declared with the `fn` keyword. The return type is specified after `->`. Use `void` for functions that do not return a value.

```rust
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}

fn main() -> void {
    const sum: i32 = add(3, 4);

    println("Sum: {}\n", sum);
}
```
### Generics

Hydra supports generics only at **compile time**
They are not type parameters like Rust 'T' or C++ templates
Instead they act as **constant values the compiler inlines**

```rust
// Takes an array of anysize, must be known at compile time 
fn print_sum(numbers: [i32, size]) -> void {
    let sum: i32 = 0;
    forEach (num in numbers) {
        sum = sum + num;
    }

    println("Sum: {}", sum);
}

fn main() -> void {
    let arr: [i32, 5] = {1, 2, 3, 4, 5};
    let bigger_arr: [i32, 10] = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10};

    print_sum(arr);
    print_sum(bigger_arr); // Compiler will inline size with the size given to the variables;
}
```

### Structs

Structs are composite data types that group together variables under a single name.
You are able declare field members along with functions inside the struct

```rust
// Struct Declaration
struct Vec3 {
    e: [f64, 3],
    
    fn new(x: f64, y: f64, z: f64) -> Vec3 {
        return Vec3 {
            e = { x, y, z };
        };
    }
}

fn main() -> void {
    let vector: Vec3 = vector::new(15, 12, 18);
    println("{}", vector) // Prints the vector

    // Accessing fields
    println("{}, {}, {}", vector.e[0], vector.e[1], vector.e[2]); // Prints the vector via field accessing
}
```

### Control Flow

#### For Loops

`for` loops iterate over a range. Incrementing or decrementing is inferred based on if start < end

* `start..end`: Exclusive range (does not include `end`).
* `start..=end`: Inclusive range (includes `end`).

```rust
// Prints numbers 0 through 9
for (i in 0..10) {
    println("{}", i);
}

// Inferred reversed iteration because start > end
// Prints numbers 5 to 0
for (i in 5..=0) {
    println("{}", i);
}
```

#### For Each Loops

`forEach` loops iterate over the elements of a collection, such as an array.

```rust
const letters: [const char, 3] = { 'a', 'b', 'c' };

forEach (letter in letters) {
    println("{}", letter);
}
```

#### While Loops

`while` loops execute as long as a condition is `true`.

```rust
let i: i32 = 0;

while (i < 5) {
    println("{}", i);
    i += 1;
}
```

#### Loop Control

* **`break`**: Exits the current loop immediately.
* **`skip`**: Skips the rest of the current iteration and proceeds to the next one (equivalent to `continue`).

### Pattern Matching

Hydra supports pattern matching via the `match` keyword.
`match` **can be used as an expression** to produce a value

```rust
let x: i32 = 10;

let check: string = match (x % 2) {
    0 => "even",
    1 => "odd"
};
```
***

## 🚀 Getting Started

This project is currently in the language specification phase. The compiler and toolchain are under active development. Stay tuned for updates!

## 🤝 Contributing

Contributions are welcome! If you have ideas for features, syntax improvements, or would like to help with the implementation, please open an issue or submit a pull request.
