# Hydra Language Grammar

This document describes the syntax and grammar of the Hydra programming language.

> **NOTE:** Hydra is under active development and this document is subject to change.
>
> Features in the **Planned Features** section are not part of the current core language design and may change before implementation.
>
> Traits are documented as part of the core language grammar because they are fundamental to Hydra's type and extension model, although compiler support for them is still under development.

---

## 1. Comments

Hydra supports C-style single-line and multi-line comments.

### Syntax

```rust
// This is a single-line comment.

/*
    This is a multi-line comment.
*/
```

Nested multi-line comments are not allowed.

---

## 2. Primitive Types

Hydra provides signed integers, unsigned integers, floating-point numbers, characters, booleans, and `void`.

### Integer Types

```text
i8
i16
i32
i64
isize

u8
u16
u32
u64
usize
```

`isize` and `usize` are pointer-sized integer types.

### Floating-Point Types

```text
f32
f64
```

### Other Primitive Types

```text
char
bool
void
```

`void` represents the absence of a value.

---

## 3. Literals

### Integer Literals

Decimal integers are written normally:

```rust
10
42
1000
```

Underscores may be used for readability:

```rust
1_000
1_000_000
```

Hexadecimal integers use the `0x` prefix:

```rust
0xFF
0xDEAD_BEEF
```

Binary integers use the `0b` prefix:

```rust
0b1010
0b1111_0000
```

Integer literals default to `i32` when no surrounding type information determines another integer type.

### Floating-Point Literals

```rust
3.14
0.5
100.0
```

Floating-point literals default to `f64` when no surrounding type information determines another floating-point type.

### Boolean Literals

```rust
true
false
```

### Character Literals

```rust
'a'
'Z'
'\n'
'\t'
'\''
'\\'
```

A character literal contains exactly one character.

### String Literals

```rust
"Hello, world!"
"Hello\nWorld"
```

Supported escapes include:

```text
\n
\r
\t
\"
\\
```

Strings are UTF-8 byte sequences and have the type:

```rust
&[u8]
```

Indexing a string therefore indexes its UTF-8 bytes rather than Unicode scalar values.

---

## 4. Variable Declarations

Hydra uses `let` for mutable bindings and `const` for immutable bindings.

Both forms require an initializer.

### Syntax

```text
let <name> [: <type>] = <expression>;
const <name> [: <type>] = <expression>;
```

### Examples

```rust
let x: i32 = 10;
x = 20;

const y: i32 = 30;
```

Type annotations may be omitted when the type can be inferred:

```rust
let count = 0;
const enabled = true;
```

`let` bindings are mutable:

```rust
let count = 0;
count = count + 1;
```

`const` bindings cannot be reassigned:

```rust
const count = 0;

// Invalid:
count = 1;
```

Hydra does not use `let mut`.

```rust
// Correct:
let value = 10;

// Not Hydra syntax:
let mut value = 10;
```

Hydra also does not support uninitialized local declarations. If a value has no useful initial state, it should generally not be declared until one exists.

---

## 5. Type Syntax

A type may be a primitive, named type, generic type, array, slice, reference, or raw pointer.

### Named Types

```rust
i32
bool
Point
core::alloc::Layout
```

### Generic Types

```rust
Vec<i32>
Pair<i32, bool>
HashMap<K, V>
```

Nested generic types are allowed:

```rust
Vec<Vec<i32>>
```

### Arrays

Fixed-size arrays use:

```text
[<type>, <length>]
```

Example:

```rust
[i32, 4]
```

### Slices

Slices use:

```text
[<type>]
```

Example:

```rust
[i32]
```

Slices are dynamically sized and are normally accessed through references:

```rust
&[i32]
&mut [i32]
```

### References

Immutable reference:

```rust
&T
```

Mutable reference:

```rust
&mut T
```

Examples:

```rust
&i32
&mut i32
&[u8]
&mut [i32]
```

### Raw Pointers

Immutable raw pointer:

```rust
*const T
```

Mutable raw pointer:

```rust
*mut T
```

Examples:

```rust
*const i32
*mut u8
```

Raw pointers must explicitly specify either `const` or `mut`.

---

## 6. Arrays and Slices

Hydra distinguishes fixed-size arrays from slices.

### Array Literals

Fixed-size array literals use braces:

```rust
const values: [i32, 3] = {10, 20, 30};
```

The length is part of the array's type.

```rust
[i32, 3]
```

### Slice Literals

Slice literals use brackets:

```rust
const values: &[i32] = [10, 20, 30];
```

Mutable slices may be created when the expected type is mutable:

```rust
const values: &mut [i32] = [10, 20, 30];
```

A slice reference conceptually contains both a pointer to its backing storage and its length.

### Indexing

Arrays and slices are indexed with square brackets:

```rust
const value = values[0];
```

Indexes must be integer values.

When an array or slice length and index are known at compile time, an out-of-bounds access may be rejected during compilation.

```rust
const values: &[i32] = [10, 20, 30];

// Compile-time error:
const value = values[3];
```

---

## 7. Operators

### Arithmetic

```text
+
-
*
/
%
```

Examples:

```rust
const a = 10 + 5;
const b = 10 - 5;
const c = 10 * 5;
const d = 10 / 5;
const e = 10 % 3;
```

### Comparison

```text
==
!=
<
<=
>
>=
```

Comparison expressions produce `bool`.

```rust
const equal = x == y;
const smaller = x < y;
```

### Logical Operators

```text
&&
||
!
```

Examples:

```rust
const result = left && right;
const other = left || right;
const inverted = !result;
```

### Numeric Negation

```rust
const negative = -value;
```

### Assignment

```text
=
+=
-=
*=
/=
%=
```

Examples:

```rust
x = 10;
x += 1;
x -= 1;
x *= 2;
x /= 2;
x %= 3;
```

The left-hand side of an assignment must be an assignable place.

Examples include:

```rust
x = 10;
point.x = 10;
array[index] = 10;
*ptr = 10;
```

### Casts

Explicit casts use `as`:

```rust
const wide = value as i64;
const ptr = reference as *const i32;
```

---

## 8. Operator Precedence

From lowest precedence to highest:

1. Assignment
2. Logical OR: `||`
3. Logical AND: `&&`
4. Equality: `==`, `!=`
5. Comparison: `<`, `<=`, `>`, `>=`
6. Addition and subtraction: `+`, `-`
7. Multiplication, division, modulo: `*`, `/`, `%`
8. Prefix operations: `!`, `-`, `&`, `&mut`, `*`
9. Calls, member access, indexing, casts, and qualified access

Parentheses may be used to explicitly control evaluation order:

```rust
const result = (a + b) * c;
```

---

## 9. References, Borrowing, and Dereferencing

### Immutable Borrow

```rust
const reference: &i32 = &value;
```

### Mutable Borrow

```rust
const reference: &mut i32 = &mut value;
```

The binding holding a reference does not itself need to be mutable in order for a mutable reference to permit mutation of its pointee.

```rust
const reference: &mut i32 = &mut value;

*reference = 42;
```

### Dereferencing

The `*` operator dereferences references and raw pointers:

```rust
const value = *reference;
*reference = 10;
```

Assignments through immutable references or immutable raw pointers are rejected.

---

## 10. Functions

Functions are declared with `fn`.

### Syntax

```text
[pub] fn <name> [<generic-parameters>] (
    <parameters>
) [-> <return-type>] [where <constraints>] {
    <body>
}
```

### Example

```rust
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}
```

A function with no explicit return type returns `void`.

```rust
fn greet() {
    println("Hello!");
}
```

The explicit form is also allowed:

```rust
fn greet() -> void {
    println("Hello!");
}
```

### Parameters

Parameters use:

```text
<name>: <type>
```

Example:

```rust
fn multiply(lhs: i32, rhs: i32) -> i32 {
    return lhs * rhs;
}
```

### Return Statements

```rust
return value;
```

A `void` function may use:

```rust
return;
```

A `void` expression may also be returned directly:

```rust
fn wrapper() -> void {
    return do_work();
}
```

---

## 11. Generic Functions

Generic parameters are declared between angle brackets immediately after the function name.

```rust
fn identity<T>(value: T) -> T {
    return value;
}
```

Multiple parameters are comma-separated:

```rust
fn convert<T, U>(value: T) -> U {
    // ...
}
```

### Generic Function Calls

Explicit generic arguments use fishtail syntax:

```rust
identity::<i32>(10);
```

Generic arguments may often be inferred from function arguments or the expected result type.

---

## 12. `where` Clauses and Trait Bounds

Trait constraints are written using `where`.

Bounds are not written directly in a generic parameter list.

```rust
fn foo<T>(value: T) -> void where T : Display {
    // ...
}
```

Multiple bounds are comma-separated:

```rust
fn foo<T>(value: T) -> void where T : Display, Debug {
    // ...
}
```

Multiple generic parameters may have separate constraints:

```rust
extension<K, V> HashMap<K, V> where K : Hash, V : Clone {
    // ...
}
```

Hydra does not use the following form:

```text
T: Display + Debug
```

and does not use Rust-style shorthand bounds in the generic declaration:

```text
<T: Display>
```

The intended Hydra form is:

```rust
<T> where T : Display, Debug
```

---

## 13. Structs

Structs define named aggregate types.

### Syntax

```text
[pub] struct <name> [<generic-parameters>] [where <constraints>] {
    [pub] <field>: <type>;
    ...
}
```

### Example

```rust
struct Point {
    x: i32;
    y: i32;
}
```

Fields are private by default.

```rust
pub struct Point {
    pub x: i32;
    pub y: i32;
}
```

Functions are not declared directly inside struct bodies. Behavior associated with a type belongs in an `extension` block.

### Generic Structs

```rust
struct Pair<T, U> {
    first: T;
    second: U;
}
```

Generic constraints may be attached with a `where` clause:

```rust
struct Container<T> where T : Clone {
    value: T;
}
```

### Struct Initialization

Struct fields are initialized using `.field = value`:

```rust
const point = Point {
    .x = 10,
    .y = 20,
};
```

A trailing comma is allowed.

### Field Access

Fields use `.`:

```rust
println("{}", point.x);
```

Assignment to a field uses the same syntax:

```rust
point.x = 30;
```

---

## 14. Extensions

Extensions attach functions to a type without placing those functions inside the type declaration.

### Syntax

```text
extension <type> {
    <functions>
}
```

### Example

```rust
struct Point {
    x: i32;
    y: i32;
}

extension Point {
    fn new(x: i32, y: i32) -> Point {
        return Point {
            .x = x,
            .y = y,
        };
    }

    fn x(&self) -> i32 {
        return self.x;
    }
}
```

Associated functions are called through the type:

```rust
const point = Point::new(10, 20);
```

Receiver methods are called through the value:

```rust
const x = point::x();
```

Hydra uses `::` for function/method dispatch and `.` for field access.

### Receiver Forms

An extension method may use one of the receiver shorthands:

```rust
self
&self
&mut self
```

Examples:

```rust
extension Counter {
    fn consume(self) -> void {
        // ...
    }

    fn get(&self) -> i32 {
        return self.value;
    }

    fn increment(&mut self) -> void {
        self.value += 1;
    }
}
```

Receiver syntax is shorthand for a first parameter involving `Self`.

### Generic Extensions

Extensions for generic types repeat the generic parameter declaration:

```rust
struct Box<T> {
    value: T;
}

extension<T> Box<T> {
    fn get(&self) -> &T {
        return &self.value;
    }
}
```

Generic extension constraints use `where`:

```rust
extension<T> Box<T> where T : Display {
    fn display(&self) -> void {
        // ...
    }
}
```

---

## 15. Traits

Traits define required behavior that types may implement.

Trait syntax is considered a fundamental part of Hydra even while compiler support is still being developed.

### Trait Declaration

The initial trait model consists of required function declarations.

```rust
trait Display {
    fn display(&self) -> void;
}
```

Trait functions do not have bodies in the initial trait implementation.

Receiver forms use the same syntax as extension methods:

```rust
trait Example {
    fn by_value(self) -> void;
    fn shared(&self) -> void;
    fn mutable(&mut self) -> void;
}
```

### Implementing a Trait

Traits are implemented using an extension:

```rust
extension Display on Point {
    fn display(&self) -> void {
        println("({}, {})", self.x, self.y);
    }
}
```

The general form is:

```text
extension <trait> on <type> {
    ...
}
```

### Trait Method Dispatch

When there is no ambiguity, trait methods participate in the normal method dispatch system:

```rust
point::display();
```

A trait function may also be addressed through the trait itself when the receiver is supplied explicitly:

```rust
Display::display(point);
```

Hydra does not fundamentally distinguish an "associated function" from a "receiver method." A receiver method is a function whose parameter list permits receiver-style dispatch.

### Explicit Trait Qualification

If multiple applicable traits or extensions expose the same member name, the implementation may be explicitly qualified:

```rust
Foo::<Printable>::show();
```

For example:

```rust
trait Printable {
    fn show(&self) -> void;
}

trait Debug {
    fn show(&self) -> void;
}
```

If `Foo` implements both traits, the desired implementation can be selected explicitly:

```rust
Foo::<Printable>::show();
Foo::<Debug>::show();
```

This syntax is primarily an ambiguity escape hatch. Ordinary code should normally be able to use:

```rust
foo::show();
```

---

## 16. Associated and Receiver Calls

Hydra uses `::` for both associated calls and receiver method calls.

### Associated Function

```rust
Vec::new();
```

### Receiver Method

```rust
vector::push(10);
```

### Generic Associated Function

Owner generic arguments appear before the associated item:

```rust
Foo::<i32>::new();
```

### Generic Function or Method

A function's own generic arguments appear immediately before its argument list:

```rust
foo::<i32>();
Foo::convert::<i32>();
value::convert::<i32>();
```

An associated function on a generic owner may therefore use both:

```rust
Foo::<T>::convert::<U>();
```

The first generic argument list belongs to `Foo`.

The second generic argument list belongs to `convert`.

---

## 17. Visibility

Top-level declarations may be public:

```rust
pub struct Point {
    pub x: i32;
}

pub fn make_point() -> Point {
    // ...
}
```

Struct fields are private by default.

Extension blocks themselves are not prefixed with `pub`. Individual extension functions may be public:

```rust
extension Point {
    pub fn new() -> Point {
        // ...
    }
}
```

---

## 18. `if` Expressions

`if` is an expression in Hydra.

### Statement-Style `if`

```rust
if condition {
    do_something();
}
```

Parentheses around the condition are optional:

```rust
if (condition) {
    do_something();
}
```

### `else`

```rust
if condition {
    do_one_thing();
} else {
    do_another_thing();
}
```

### `else if`

```rust
if first {
    // ...
} else if second {
    // ...
} else {
    // ...
}
```

### Value-Producing `if`

An `if` expression may produce a value.

Every control-flow path must produce a compatible value, so a value-producing `if` requires an `else`.

```rust
const value: i32 = if condition {
    10;
} else {
    20;
};
```

The final expression statement in each branch supplies the branch value.

The branches must produce compatible types.

---

## 19. `while` Loops

### Syntax

```rust
while condition {
    // ...
}
```

Parentheses are optional:

```rust
while (condition) {
    // ...
}
```

Example:

```rust
let i = 0;

while i < 10 {
    println("{}", i);
    i += 1;
}
```

---

## 20. Range `for` Loops

Hydra provides range-based `for` loops.

### Exclusive Range

```rust
for i in 0..10 {
    println("{}", i);
}
```

This iterates from `0` up to but not including `10`.

### Inclusive Range

```rust
for i in 0..=10 {
    println("{}", i);
}
```

This includes the ending value.

### Descending Ranges

Hydra determines the direction of a range from its starting and ending values.

```rust
for i in 10..0 {
    println("{}", i);
}
```

The loop automatically decrements for a descending range.

Likewise:

```rust
for i in 10..=0 {
    println("{}", i);
}
```

iterates downward and includes `0`.

The ending bound is evaluated once before iteration begins.

Parentheses around the range expression are optional:

```rust
for (i in 0..10) {
    // ...
}
```

---

## 21. `foreach` Loops

`foreach` iterates over the elements of an aggregate collection.

The current form operates on arrays.

### Syntax

```rust
foreach value in values {
    // ...
}
```

Example:

```rust
const values: [i32, 3] = {10, 20, 30};

foreach value in values {
    println("{}", value);
}
```

Parentheses are optional:

```rust
foreach (value in values) {
    // ...
}
```

---

## 22. `break` and `continue`

### `break`

```rust
break;
```

### `continue`

```rust
continue;
```

### Conditional `break`

Hydra allows a condition directly on `break`:

```rust
break if done;
```

Parentheses may also be used:

```rust
break if (done);
```

### Conditional `continue`

```rust
continue if skip;
```

or:

```rust
continue if (skip);
```

These are equivalent in meaning to placing the operation inside an `if`.

```rust
if done {
    break;
}
```

---

## 23. Includes and Module Paths

Hydra source files form modules through their filesystem/module path.

External symbols are brought into scope with `include`.

### Include a Module

```rust
include core::mem;
```

### Include a Nested Module

```rust
include core::alloc::Layout;
```

### Selective Include

```rust
include core::mem::{size_of, align_of};
```

A trailing comma is allowed:

```rust
include core::mem::{
    size_of,
    align_of,
};
```

### Aliased Include

A module include may be given a local alias:

```rust
include some::long::module as short;
```

It may then be addressed through the alias:

```rust
short::function();
```

Hydra does not use a `mod` declaration syntax.

---

## 24. Paths

Names in another namespace or module use `::`.

```rust
core::mem::size_of
alloc::vec::Vec
foo::bar::Baz
```

Paths are also used for associated functions and methods:

```rust
Vec::new();
vector::push(value);
```

Field access remains distinct and uses `.`:

```rust
point.x
```

---

## 25. External Functions

External functions use `extern fn` and do not contain a Hydra body.

```rust
extern fn external_function(value: i32) -> i32;
```

Public external declarations are allowed:

```rust
pub extern fn external_function(value: i32) -> i32;
```

---

## 26. Annotations

Annotations use `#[...]` syntax and precede the declaration they apply to.

### Syntax

```rust
#[annotation]
fn foo() -> void {
    // ...
}
```

Annotations may syntactically accept string arguments:

```rust
#[annotation("value")]
fn foo() -> void {
    // ...
}
```

### Inline Functions

```rust
#[inline]
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}
```

`#[inline]` marks a function as eligible for compiler inlining.

### Compiler Intrinsics

Compiler intrinsics are declared using:

```rust
#[intrinsic]
fn intrinsic_name<T>(...) -> ...;
```

Intrinsic declarations are compiler-controlled and must match the signature expected for that intrinsic.

They are primarily used by Hydra's core library rather than ordinary application code.

---

## 27. Entry Point

A Hydra executable program requires:

```rust
fn main() -> void {
    // ...
}
```

`main` is the program entry point.

---

# Planned Features

The features in this section are intended or under consideration but are not yet part of Hydra's stable core grammar.

Their exact syntax and semantics may change.

---

## P1. Generic Traits

Traits are initially being introduced without generic trait parameters.

Generic traits are planned.

Possible syntax:

```rust
trait Iterator<T> {
    fn next(&mut self) -> T;
}
```

A generic trait implementation would use the normal extension generic syntax.

For example:

```rust
extension<T> SomeTrait<T> on SomeType<T> {
    // ...
}
```

The exact implementation rules remain subject to development.

---

## P2. Default Trait Methods

Traits initially consist of required function declarations:

```rust
trait Display {
    fn display(&self) -> void;
}
```

Default method implementations are planned.

The intended direction is:

```rust
trait Display {
    fn display(&self) -> void;

    fn debug_display(&self) -> void {
        // default implementation
    }
}
```

A type implementing the trait would only need to provide functions that do not already have an acceptable default implementation.

---

## P3. Forbidden Trait Extensions

Hydra may provide an explicit mechanism for forbidding a trait implementation on a type.

The currently planned syntax is:

```rust
forbid extension Clone on Foo;
```

For generic types, the extension generics must still be declared explicitly:

```rust
forbid extension<T> Clone on Foo<T>;
```

Another example:

```rust
forbid extension<K, V> SomeTrait on HashMap<K, V>;
```

This syntax is intentionally considered provisional and may change before implementation.

---

## P4. Optional Types

Hydra plans to support optional values directly in the type syntax.

### Syntax

```rust
T?
```

Example:

```rust
const value: i32? = find_value();
```

An optional represents either:

* a value of `T`, or
* `None`.

Absence is written exactly as:

```rust
None
```

Hydra does not use a `Some(value)` constructor.

A present optional value is represented directly by the underlying value:

```rust
fn maybe_value(condition: bool) -> i32? {
    if condition {
        return 10;
    }

    return None;
}
```

Conceptually, once the `None` case has been eliminated, the only remaining possibility is the value itself.

This style may be described as **apophatic programming**: instead of wrapping the positive case in a constructor, the program eliminates the negative case and what remains is the value.

---

## P5. `match`

Pattern matching is planned.

One major use will be exhaustive handling of optional values.

The intended shape is:

```rust
match value {
    None => handle_none(),
    value => use_value(value),
}
```

For an optional `T?`, handling `None` leaves the remaining arm as the contained `T`.

There is no required `Some(value)` pattern.

The complete pattern grammar may expand as `match` is implemented.

---

## P6. `orelse`

`orelse` is planned as an expression-level way to provide an alternative when an optional has no value.

Conceptually, it behaves similarly to an `unwrap_or_else` operation.

Example:

```rust
const value: i32? = find_value();

const result = value orelse 0;
```

Control flow may also be usable as the alternative:

```rust
const result = value orelse return 0;
```

When the optional contains a value, the expression evaluates to the underlying value.

When it contains `None`, the expression after `orelse` is evaluated instead.

---

## P7. Error Unions

Hydra plans to represent fallible return values using an error-union type.

### Syntax

```rust
T | E
```

This reads as:

> `T` or error `E`

The left side is always the successful value type.

The right side is always the error type.

Example:

```rust
fn parse(value: &[u8]) -> i32 | ParseError {
    // ...
}
```

An error union contains exactly one success type and one error type.

Hydra does not currently plan a form such as:

```text
T | E1 | E2
```

A program requiring multiple error variants should represent those variants through a single error type.

---

## P8. Error Propagation

A postfix `!` operator is currently the preferred direction for error propagation, although the syntax is not yet final.

Possible syntax:

```rust
const value = parse(input)!;
```

The intended meaning is:

* if the expression succeeds, produce its `T`;
* if the expression produces `E`, return that error from the current function.

This would be Hydra's equivalent of a propagation operator.

It does not conflict grammatically with logical negation because the operations occur in different positions:

```rust
!condition
```

is prefix logical negation, while:

```rust
parse()!
```

would be postfix error propagation.

The postfix propagation syntax remains provisional.

---

## P9. `never`

Hydra plans to provide a bottom type named:

```rust
never
```

A function that never returns may use:

```rust
fn panic(message: &[u8]) -> never {
    // ...
}
```

`never` is coercible to every type.

This permits expressions that terminate control flow to appear where another value would otherwise be required.

For example:

```rust
const value = optional orelse panic("missing value");
```

The `panic` expression has type `never`, which can satisfy the surrounding expression's type requirement because execution never continues through that branch.

---

## P10. `panic`

A standard panic facility is planned alongside the `never` type.

Conceptually:

```rust
fn panic(message: &[u8]) -> never;
```

A panic terminates normal control flow and therefore returns `never`.

Runtime failures such as dynamic out-of-bounds indexing may eventually use this mechanism.

---

## P11. Range-Based Array and Slice Indexing

Hydra currently uses ranges in `for` loops:

```rust
0..10
0..=10
```

Using the same range syntax for array and slice indexing is under consideration.

Possible forms include:

```rust
values[start..end]
values[start..=end]
values[..end]
values[start..]
```

The exact semantics, resulting types, mutability behavior, and bounds rules have not yet been finalized.

This feature is exploratory rather than fixed language syntax.

---

# Grammar Summary

The following is a compact overview of Hydra's primary syntax.

```text
program
    := item*

item
    := include
     | function
     | extern-function
     | struct
     | trait
     | extension

include
    := "include" path ";"
     | "include" path "as" IDENTIFIER ";"
     | "include" path "::" "{"
            IDENTIFIER ("," IDENTIFIER)* [","]
        "}" ";"

function
    := annotation*
       ["pub"]
       "fn" IDENTIFIER
       generic-params?
       "(" parameters? ")"
       ("->" type)?
       where-clause?
       block

extern-function
    := annotation*
       ["pub"]
       "extern" "fn" IDENTIFIER
       generic-params?
       "(" parameters? ")"
       ("->" type)?
       where-clause?
       ";"

struct
    := ["pub"]
       "struct" IDENTIFIER
       generic-params?
       where-clause?
       "{"
           struct-field*
       "}"

struct-field
    := ["pub"] IDENTIFIER ":" type ";"

extension
    := "extension"
       generic-params?
       type
       where-clause?
       "{"
           extension-function*
       "}"

trait-extension
    := "extension"
       generic-params?
       type "on" type
       where-clause?
       "{"
           extension-function*
       "}"

trait
    := ["pub"]
       "trait" IDENTIFIER
       "{"
           trait-function*
       "}"

trait-function
    := "fn" IDENTIFIER
       generic-params?
       "(" parameters? ")"
       ("->" type)?
       where-clause?
       ";"

generic-params
    := "<" IDENTIFIER ("," IDENTIFIER)* ">"

parameters
    := parameter ("," parameter)*

parameter
    := IDENTIFIER ":" type
     | "self"
     | "&self"
     | "&mut self"

where-clause
    := "where" constraint-list

variable-declaration
    := "let" IDENTIFIER (":" type)? "=" expression ";"
     | "const" IDENTIFIER (":" type)? "=" expression ";"

return
    := "return" expression? ";"

break
    := "break" ("if" expression)? ";"

continue
    := "continue" ("if" expression)? ";"

if-expression
    := "if" expression block
       ("else" (if-expression | block))?

while-expression
    := "while" expression block

for-expression
    := "for" IDENTIFIER "in"
       expression (".." | "..=") expression
       block

foreach-expression
    := "foreach" IDENTIFIER "in" expression block

type
    := path
     | path "<" type-list ">"
     | "[" type "," INTEGER "]"
     | "[" type "]"
     | "&" type
     | "&mut" type
     | "*const" type
     | "*mut" type

expression
    := assignment-expression

assignment-expression
    := logical-or
       (
           ("=" | "+=" | "-=" | "*=" | "/=" | "%=")
           assignment-expression
       )?

logical-or
    := logical-and ("||" logical-and)*

logical-and
    := equality ("&&" equality)*

equality
    := comparison (("==" | "!=") comparison)*

comparison
    := additive
       (("<" | "<=" | ">" | ">=") additive)*

additive
    := multiplicative (("+" | "-") multiplicative)*

multiplicative
    := unary (("*" | "/" | "%") unary)*

unary
    := ("!" | "-" | "&" | "&mut" | "*") unary
     | postfix

postfix
    := primary postfix-operation*

postfix-operation
    := "(" arguments? ")"
     | "::" IDENTIFIER
     | "::<" type-list ">"
     | "." IDENTIFIER
     | "[" expression "]"
     | "as" type
     | struct-initializer

array-literal
    := "{" expression-list? "}"

slice-literal
    := "[" expression-list? "]"

struct-initializer
    := "{"
         ("." IDENTIFIER "=" expression)
         ("," "." IDENTIFIER "=" expression)*
         [","]
       "}"

annotation
    := "#" "[" IDENTIFIER
       ("(" STRING ("," STRING)* ")")?
       "]"
```

Planned additions include:

```text
optional-type
    := type "?"

error-union
    := type "|" type

match-expression
    := "match" expression "{"
           match-arm*
       "}"

match-arm
    := pattern "=>" expression

orelse-expression
    := expression "orelse" expression

never-type
    := "never"

forbidden-extension
    := "forbid" "extension"
       generic-params?
       type "on" type
       ";"
```

The exact grammar for planned features remains subject to change.
