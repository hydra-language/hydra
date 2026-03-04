Hydra Language Grammar
======================

This document outlines the grammar and syntax for the Hydra programming language. All examples demonstrate the intended way to write Hydra code.

NOTE: this is subject to change

* * *

1\. Comments
------------

Comments are used for annotating code and are ignored by the compiler. Hydra uses C-style single-line comments.

**Syntax**:

    // This is a single-line comment.

    /* This is a multi
        line comment */

Nested comments are NOT allowed

* * *

2\. Variable Declarations
-------------------------

Variables are declared using the **`let`** keyword for mutable variables and **`const`** for immutable variables. \
Type annotations are not mandatory for stack allocated variables.

**Syntax**:

    <let | const> <variable_name>: <type?> = <initial_value>;

**Examples**:
```rust
// A mutable 32-bit integer that can be reassigned.
let x: i32 = 10;
x = 22;
    
// An immutable 32-bit float that cannot be reassigned.
const PI: f32 = 3.14;
```
* * *

3\. Data Types
--------------

### Primitives

Hydra includes a standard set of primitive types.

*   **Signed Integers**: `isize`, `i8`, `i16`, `i32`, `i64`
*   **Unsigned Integers**: `usize`, `u8`, `u16`, `u32`, `u64`
*   **Floating-Point**: `f32`, `f64`
*   **Character**: `char` (e.g., `'c'`)
*   **Boolean**: `bool` (`true` or `false`)

### Arrays

Arrays have a fixed size known at compile time and cannot grow

**Syntax**:

    <let | const> <name>: [<type>, <size>] = { <elements> };

**Examples**:
```rust
// A mutable array with mutable elements.
let a: [i32, 3] = { 1, 2, 3 };
a[0] = 100; // OK
    
// An immutable array with mutable elements.
const c: [i64, 3] = { 1, 2, 3 };
c = { 4, 5, 6 }; // ERROR: binding 'c' is const.
```

4\. Memory
-----------

Memory layout and memory safety are still being defined. The goal is something similar to Rust. \
For now, everything is allocated on the stack

---

5\. Structs and Extensions
--------------------------

Structs are user-defined types that group related data and functions. \
Structs can have regular data fields, constants and functions. \
Struct constants follow the standard for writing a const variable

Extensions are a way to override `trait` functions for user defined types

**Syntax**:

```rust
    struct <StructName> {
        <field_name>: <type>;
        const name: type = val; 
        
        fn <method_name>(<parameters>) -> <return_type> {
            // Method body
        }
    }

    extension <trait> on <user_type> {
        fn <trait>(&self) -> anytype {
            /* Your override here */
        }
    }
```
**Example**:

```rust
struct Vec3 {
    vector: [f64, 3],
        
    fn new(x: f64, y: f64, z: f64) -> Vec3 {
        return Vec3 {
            vector = { x, y, z };
        };
    }
}

extension Copy on Vec3 {
    fn copy(&self, dest: anytype, len: anysize) -> anytype {
        /* Your override here */
    }
}
    
// Instantiation and use
fn main() -> void {
    let vector: Vec3 = Vec3::new(15.0, 12.0, 18.0);
    println("{}", vector.e[0]); // Accessing a field
}
```
* * *

6\. Functions and Generics
--------------------------

Functions are defined with the **`fn`** keyword, mandatory type annotations for parameters, and a specified return type. Use **`void`** for functions that do not return a value.

**Syntax**:

    fn <function_name>(<param1>: <type1>, <param2>: <type2>, ...) -> <return_type> {
        // Function body
        return <value>;
    }

A very simple example
```rust
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}

fn main() -> void {
    const sum: i32 = add(5, 3);
}
```

### Compile-Time Generics

Hydra supports compile-time generics: \
`anysize` - a constant value the compiler inlines \
`anytype` - another constant value the compiler inlines with the type of the variable associated with in during the type check phase of compilation

### Anysize
The `anysize` generic allows this function to accept an i32 array of any length.
```rust
fn print_sum(numbers: [i32, anysize]) -> void {
    let sum: i32 = 0;
    foreach (num in numbers) {
        sum += num;
    }
    println("Sum: {}", sum);
}
```
However, keep in mind, this does NOT mean your array can be dynamically resized. \
Once the compiler finds the size of the array, all other references to it will also be of the same size. \
If the compiler detects an attempt to resize the array, your program will not compile

### Anytype
The `anytype` generic allows a function to accept a value of any type
```rust
fn get_type(value: anytype) -> void {
    const type = typeof(value);
    
    println("Type: {}, Value: {}", type, value);  
}

fn main() -> void {
    get_type(10);
    get_type('c');    
}
```
* * *

7\. Modules
-----------
### Include
The `include` keyword allows you to import external files \
To separate namespaces, use `module::submodule` and if you would like to import everything use `module::*`
```rust
// brings in Vec from the standard library
include std::Vec;

fn main() -> void {
    let vec: Vec<i32> = Vec::new();
    vec::push(1);
    
    println("{}", vec::get(1));
}
```

8\. Control Flow
----------------

### For Loops

The **`for`** loop iterates over a numerical range. The direction (incrementing or decrementing) is automatically inferred.

*   `start..end`: Exclusive range (up to, but not including, `end`).
*   `start..=end`: Inclusive range (up to and including `end`).

**Syntax**:

    for (<variable> in <range>) {
        // Loop body
    }

**Examples**:
```rust
fn main() -> void {
    // Prints 0, 1, 2, ... 9
    for (i in 0..10) {
        println("{}", i);
    }
    
    // Inferred reverse direction. Prints 5, 4, 3, 2, 1, 0
    for (i in 5..=0) {
        println("{}", i);
    }
```
### For Each Loops

The **`foreach`** loop iterates over every element in a collection, such as an array.

**Syntax**:

    foreach (<variable> in <collection>) {
        // Loop body
    }

**Example**:
```rust
const letters: [const char, 3] = { 'a', 'b', 'c' };
foreach (letter in letters) {
    println("{}", letter);
}
```
### While Loops

The **`while`** loop executes repeatedly as long as its condition remains `true`.

**Syntax**:

    while (<condition>) {
        // Loop body
    }

**Example**:
```rust
let i: i32 = 0;
while (i < 5) {
    i++;
}
```
### Loop Control

*   **`break`**: Exits the current loop entirely.
*   **`break if (condition)`**: Exits the loop if the condition evaluates to true
*   **`continue if (condition)`**: Skips the remainder of the current iteration and continues to the next one if condition is true

This skips the traditional wrapping of `continue` or `break` in an `if` statement for a single expression \
You may choose to do the traditional `if cond { continue \ break }` for a single expression but the standard is `break if (cond)` or `continue if (cond)` \
If your statement executes multiple lines of code, then the traditional way is necessary

```rust
// prints i and skips even numbers
for (i in 0..10) {
    println("{}", i);
    continue if (i % 2 == 0);
}

for (i in 0..20) {
    println("{}", i); 
    break if (i % 5 == 0 && i != 0);
}
```

* * *

9\. Pattern Matching
--------------------

The **`match`** keyword provides pattern matching. It can be used as an expression to return a value.

**Syntax**:

    let <result> = match (<expression>) {
        <pattern1> => <value1>,
        <pattern2> => <value2>,
        ...
    };

**Example**:
```rust
let x: i32 = 10;
let check: string = match (x % 2) {
    0 => "even",
    1 => "odd"
};
```

