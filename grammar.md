Hydra Language Grammar
======================

This document outlines the grammar and syntax for the Hydra programming language. All examples demonstrate the intended way to write Hydra code.

* * *

1\. Comments
------------

Comments are used for annotating code and are ignored by the compiler. Hydra uses C-style single-line comments.

**Syntax**:

    // This is a single-line comment.
    /* This is a multi
        line comment */

* * *

2\. Variable Declarations
-------------------------

Variables are declared using the **`let`** keyword for mutable variables and **`const`** for immutable constants. Type annotations are not mandatory for stack allocated variables.

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

Arrays have a fixed size and can have mutable or immutable elements, independent of the array's own mutability.

**Syntax**:

    <let | const> <name>: [<const?> <type>, <size>] = { <elements> };

**Examples**:
```rust
// A mutable array with mutable elements.
let a: [i32, 3] = { 1, 2, 3 };
a[0] = 100; // OK
    
// A mutable array with immutable elements.
let b: [const char, 5] = { 'h', 'e', 'l', 'l', 'o' };
b[0] = 'j'; // ERROR: elements are const.
    
// An immutable array with mutable elements.
const c: [i64, 3] = { 1, 2, 3 };
c = { 4, 5, 6 }; // ERROR: binding 'c' is const.
    
// A fully immutable array.
const d: [const char, 2] = { 'x', 'y' };
```

4\. Memory
-----------

Memory works in a little bit of a complicated way. 
The idea is RAII for stack allocated items (Resource Aqcuistion Is Initialization)
and ARC (Automatic Reference Counting) for heap allocated.

The idea behind this is to eliminate the need for a super strict borrow checker
like Rust has and whilst having memory safety without a garbage collector.

### Stack
In Hydra, all primitive types and their array equivalents are stack allocated and are managed by RAII

```rust
fn main() -> void {
    const x: i32 = 10; // x is an primtive i32 and is allocated 4 bytes on the stack
    const arr: [f64, 5] = {3.14, 3.14, 3.14, 3.14, 3.14}; // arr is an array of 5 f64s and is allocated 40 bytes on the stack
}
```

### Heap
Reference types or non primitive types are a bit different.
These are allocated on the heap by wrapping them in pipes (`|`).
These give a reference to the stack managed by ARC.

```rust
fn main() -> void {
    // this is a heap allocated Vec (dynamic array)
    // under the hood, variables are wrapped in | |
    // specifically, the struct returned in new is allocated the heap,
    // which in turn, makes anything that depends on new also heap allocated
    let vec: Vec<i32> = Vec<i32>::new();
    vec::push(5);
}
```

Suppose you wanted to make your own String representation.
It could look like this, for example:
```rust
struct String {
    data: [const char, anysize],
    len: usize,
    capacity: usize,

    fn new(data: &[const char, anysize]) -> |String| {
        let len: usize = data::length();
        let capacity = &len;

        // there is no need to wrap String in | | in the return statement
        // as the return type of the function is a heap allocation
        return String { 
            data = data,
            len = len,
            capacity = len
        };
    }
}
```

5\. Structs and Extensions
-----------

Structs are user-defined types that group related data and functions.
Extensions are a way to override `trait` functions for user defined types

**Syntax**:

    struct <StructName> {
        <field_name>: <type>,
        ...
        fn <method_name>(<parameters>) -> <return_type> {
            // Method body
        }
    }

    extension <trait> on <user_type> {
        fn <trait>(&self) -> anytype {
            /* Your override here */
        }
    }

**Example**:
```rust
struct Vec3 {
    e: [f64, 3],
        
    fn new(x: f64, y: f64, z: f64) -> Vec3 {
        return Vec3 {
            e = { x, y, z };
        };
    }
}

extension Copy on Vec3 {
    fn copy(&self, dest: anytype, len: anysize) -> anytype {
        /* Your override here */
    }
}
    
// Instantiation and use
let vector: Vec3 = Vec3::new(15.0, 12.0, 18.0);
println("{}", vector.e[0]); // Accessing a field
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

Lets say you'd like to take an i64 and do arithmetic with an i32.
You would need to cast the smaller type to the bigger type
using the **`as`** keyword.

There are two ways of doing this:
Cast the value in the return statement or cast the parameter
when the function is called.

The return type, in this instance, needs to match the bigger type.
```rust
fn add(a: i32, b: i64) -> i64 {
    return a as i64 + b; // cast in return statement
}

fn subtract(a: i32, b: i64) -> i64 {
    return a - b;
}

fn main() -> void {
    const sum: i64 = add(5, 6);
    const difference: i64 = subtract(10 as i64, 5);

    println("{}", sum);
    println("{}", difference);
}
```

### Compile-Time Generics

Hydra supports compile-time generics:
    **`anysize`** - a constant value the compiler inlines
    **`anytype`** - another constant value the compiler inlines with the type of the variable associated with in during the type check phase of compilation

**Example**:
```rust
// The 'anysize' parameter allows this function to accept an i32 array of any length.
fn print_sum(numbers: [i32, anysize]) -> void {
    let sum: i32 = 0;
    foreach (num in numbers) {
        sum = sum + num;
    }
    println("Sum: {}", sum);
}

fn identity(x: anytype) -> anytype {
    return x::typeof();
}
    
fn main() -> void {
    let size_5_arr: [i32, 5] = {1, 2, 3, 4, 5};
    let size_10_arr: [i32, 10] = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10};
    
    print_sum(size_5_arr);      // Compiler inlines 'size' as 5
    print_sum(size_10_arr);     // Compiler inlines 'size' as 10

    const x = 22;                   // inferring type as i32
    const typeof_x = identity(x);   // returns type i32

    // NOTE: if you annotate the variable with a type
    // the function will just automatically return that
    // type, it will not perform analysis. For example:
    let y: f64 = 3.14;
    const typeof_y = identity(y);

    // You could also annotate the variable holding
    // the function call. If the type you annotate
    // with doesnt match the type passed, a error
    // will occur. This is the problem `anytype`
    // solves. For example:
    let z = "Hello";
    const typeof_z_wrong: i32 = identity(z);      // This will throw an error
    const typeof_z_right: anytype = identity(z);
    
    println("{}", typeof_x);
    println("{}", typeof_y);
    println("{}", typeof_z_right);
}
```
* * *

7\. Control Flow
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
    i += 1;
}
```
### Loop Control

*   **`break`**: Exits the current loop entirely.
*   **`break if (<condition>)`: Exits the loop if the condition evaluates to true
*   **`continue if (condition)`**: Skips the remainder of the current iteration and continues to the next one if condition is true

This skips the traditional wrapping of `continue` or `break` in an `if` statement
You may also run a block before the control action, see below

```rust
// prints i and skips even numbers
for (i in 0..10) {
    continue if (i % 2 == 0) {
        println("{}", i);
    };
}

for (i in 0..20) {
    break if (i % 7 == 0 && i != 0) {
        println("{}", i);
    };
}
```

* * *

8\. Pattern Matching
--------------------

The **`match`** keyword provides powerful pattern matching. It can be used as an expression to return a value.

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
