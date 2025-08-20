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

Variables are declared using the **`let`** keyword for mutable variables and **`const`** for immutable constants. Type annotations are mandatory.

**Syntax**:

    <let | const> <variable_name>: <type> = <initial_value>;

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

*   **Signed Integers**: `i8`, `i16`, `i32`, `i64`
*   **Unsigned Integers**: `u8`, `u16`, `u32`, `u64`
*   **Floating-Point**: `f32`, `f64`
*   **Character**: `char` (e.g., `'c'`)
*   **Boolean**: `bool` (`true` or `false`)
*   **String**: `string` (e.g., `"hello"`)

### Arrays

Arrays have a fixed size and can have mutable or immutable elements, independent of the array's own mutability.

**Syntax**:

    <let | const> <name>: [<element_mutability?> <type>, <size>] = { <elements> };

*   `<element_mutability?>` is an optional **`const`** keyword to make the elements immutable.

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
### Array Slicing

Slicing creates a view or a copy of a portion of an array.

**Read-only Slice Syntax**:

    let <slice_name>: [<type>, <size>] = &<array_name>[<start>..<end>];

**Mutable Heap Slice Syntax**:

    let <slice_name>: [<type>, <size>] = |<array_name>|[<start>..<end>];

*   The `|...|` syntax allocates the new slice on the heap.

**Examples**:
```rust
let arr: [i32, 5] = {1, 2, 3, 4, 5};
    
// Create a read-only slice referencing the original array.
let read_only_slice: [i32, 3] = &arr[0..2];
    
// Create a new, mutable slice on the heap.
let heap_slice: [i32, 2] = |arr|[3..5];
heap_slice[0] = 40; // OK
```
* * *

4\. Structs
-----------

Structs are user-defined types that group related data and functions.

**Syntax**:

    struct <StructName> {
        <field_name>: <type>,
        ...
        fn <method_name>(<parameters>) -> <return_type> {
            // Method body
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
    
// Instantiation and use
let vector: Vec3 = Vec3::new(15.0, 12.0, 18.0);
println("{}", vector.e[0]); // Accessing a field
```
* * *

5\. Functions and Generics
--------------------------

Functions are defined with the **`fn`** keyword, mandatory type annotations for parameters, and a specified return type. Use **`void`** for functions that do not return a value.

**Syntax**:

    fn <function_name>(<param1>: <type1>, <param2>: <type2>) -> <return_type> {
        // Function body
        return <value>;
    }

### Compile-Time Generics

Hydra supports compile-time generics, where a generic parameter like **`size`** acts as a constant value that the compiler inlines based on the provided arguments.

**Example**:
```rust
// The 'size' parameter allows this function to accept an i32 array of any length.
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
    
    print_sum(arr);        // Compiler inlines 'size' as 5
    print_sum(bigger_arr); // Compiler inlines 'size' as 10
}
```
* * *

6\. Control Flow
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
// Prints 0, 1, 2, ..., 9
for (i in 0..10) {
        println("{}", i);
    }
    
    // Inferred reverse direction. Prints 5, 4, 3, 2, 1, 0
    for (i in 5..=0) {
        println("{}", i);
    }
```
### For Each Loops

The **`forEach`** loop iterates over every element in a collection, such as an array.

**Syntax**:

    forEach (<variable> in <collection>) {
        // Loop body
    }

**Example**:
```rust
const letters: [const char, 3] = { 'a', 'b', 'c' };
forEach (letter in letters) {
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
*   **`skip`**: Skips the remainder of the current iteration and continues to the next one (like `continue` in other languages).

* * *

7\. Pattern Matching
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
