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

Variables are declared using the **`let`** keyword for mutable variables and **`const`** for immutable constants. Type annotations are not mandatory for variables.

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

*   **Signed Integers**: `i8`, `i16`, `i32`, `i64`
*   **Unsigned Integers**: `u8`, `u16`, `u32`, `u64`
*   **Floating-Point**: `f32`, `f64`
*   **Character**: `char` (e.g., `'c'`)
*   **Boolean**: `bool` (`true` or `false`)

### Arrays

Arrays have a fixed size and can have mutable or immutable elements, independent of the array's own mutability.

**Syntax**:

    <let | const> <name>: [<const?> <type>, <size>] = { <elements> };

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

**Array Slice Syntax**:

    <let | const> <name>: [<const?> <type>, <size>] = &<original>[<start>..<end>];

    <let | const> <name>: [<const?> <type>, <size>] = |<original>|[<start>..<end>];

*   `&` creates a reference slice (no allocation)
*   The `|...|` syntax allocates the new slice on the heap. Elements are copied and independent of the original array
*   The rules of arrays layed out above still apply here.

* * **Mutability Rules for Reference Slices (`&`)**:

1. Edits are allowed **only if**:
   - The original array is mutable (`let`), and  
   - The elements are mutable (no `const` in element type), and  
   - The slice binding itself is mutable (`let`).
2. Immutable arrays or arrays with `const` elements cannot be modified through a reference slice, even if the slice is bound with `let`.  

**Mutability Rules for Heap Slices (`|...|`)**:

1. Heap slices copy the elements into new memory.  
2. Mutability of a heap slice is independent of the original array:
   - `let` heap slice → editable elements  
   - `const` heap slice → read-only elements  
3. This allows you to take a `const` array with `const` elements and produce a fully mutable heap slice.

**Examples**:

```rust
let arr: [i32, 5] = {1, 2, 3, 4, 5};

// Reference slice of mutable elements
let ref_slice: [i32, 2] = &arr[1..3];
ref_slice[0] = 10; // ✅ OK

// Const reference slice
const const_slice: [i32, 2] = &arr[1..3];
const_slice[0] = 20; // ❌ ERROR: slice binding is const

// Reference slice from const array
const arr2: [i32, 5] = {1,2,3,4,5};
let ref_slice2: [i32, 2] = &arr2[0..2];
ref_slice2[0] = 1; // ❌ ERROR: original array is const

// Heap slice (independent copy)
let heap_slice: [i32, 3] = |arr2|[1..4];
heap_slice[0] = 99; // ✅ OK*
```

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

    fn <function_name>(<param1>: <type1>, <param2>: <type2>, ...) -> <return_type> {
        // Function body
        return <value>;
    }

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
