# Language Specification

## Primitives
- `i8`
- `i16`
- `i32`
- `i64`
- `u8`
- `u16`
- `u32`
- `u64`
- `f32`
- `f64`
- `char` → delimited by single quotes
- `string` (pseudo primitive) → delimited by double quotes
- `boolean`
- array equivalents

## Keywords (so far)
- `let` → for mutables
- `const` → for immutables
- `struct`
- `fn`
- `return`
- `void`
- `in`
- `for`
- `if`
- `else`
- `else if`
- `forEach`
- `while`
- `break`
- `skip` (my version of continue)
- `include` (for imports)
- `typedef` (for aliasing)
- `None` (for catching of nothing from optional types)

## Intended Use

### Basic Variable Declaration
```
let x: i32 = 10;

    Syntax: let <name>: <type> (must be present) = <value>;

    Mutable → can be changed

Example:

x = 22;  // good, no compile time error

Using const binds the reference:

const y: f32 = 3.14;

    Syntax: const <name>: <type> (must be present) = <value>;

    Immutable → cannot be changed

Example:

y = 41.3;  // bad, compile time error

Example string:

let str: string = "string";
```

### Arrays
```
Declaration:

<binding> <name>: [<mutability?> <type>, <size>] = { ... };

    binding = 'const' or 'let'

    mutability? = 'const' or nothing at allocated element type

    type = primitives or custom type

    Im thinking of doing this:
    { ... } ) is the placeholder for an empty array (ellipsis)
    whitespace doesnt matter so {...} or {   ... } also works

Examples:

let a: [i32, 3] = { 1, 2, 3 };             // fully mutable

let b: [const char, 5] = { 'h', 'e', 'l', 'l', 'o' };  // immutable elements

const c: [i64, 3] = { 1, 2, 3 };           // immutable binding

const d: [const char, 2] = { 'x', 'y' };   // fully immutable


Array slicing comes with the language with the special := operator:

Declaration:
    let arr: [i32, 5] = {1, 2, 3, 4, 5};
    let slice: [i32, 3] = &arr[0..2]; // tells the compiler that you want to make a readonly slice of the original
                                    // in order to make an editable one you must make a copy on the heap, see below
    
    // Heap version
    let heap_slice: [i32, 3] := |arr|[0..2];

    With this you can now do operations on it
    ie. changing values via index, maybe a .to_vec() will be added in the stdlib

```

### Functions
```
Declaration:

fn <name>(<params>) -> <return_type> {
    <code>
}

Examples:

fn add(a: i32, b: i32) -> i32 {
    return a + b;
}

fn proc_nums(nums: [i32, N]) -> void {
    <code>
}
    NOTE: N is a generic type parameter ONLY used in functions where the size of the array being passed can vary.
    The compiler will inline this in the original function call for every time the function is called with different sized arrays.
    This is called monomorphization.
```

### Structs
```
Declaration:

struct <name> {
    <fields>...

    <name>: <type>;
}

Examples:

struct Student {
    name: string;
    age: i32;
    gpa: f32;
};

derive is a huge maybe that I might add

derive Student {
    fn get_age(&self) -> i32 {
        return self.age;
    }

    fn new(name: string, age: i32, gpa: f32) -> Student {
        return Student {
            name = name;
            age = age;
            gpa = gpa;
        };
    }
}

fn main() -> void {
    let student: Student = Student {
        name = "Joseph";
        age = 16;
        gpa = 3.92;
    };

    Effectively creating a new instance of student
    In theory, programmer could make a function called new in the derive that does this (see above)
    Im not doing that cuz thats OOP and OOP is gay
    Yes what im doing now is OOP but its OOP without being explicitly OOP so not gay
    Sure its the same thing but not gay cuz I said so

    let age: i32 = get_age(&student);

    No need for method like calling as the only time get_age can be used is in reference to Student unless get_age() defined elsewhere
    if defined elsewhere, maybe throw comptime error? something like can not determine which function to call?
        - Solution
            Enforce a qualifier like <scope>::<function>() for scoped functions
            ie. Student::get_age()

            Default to free standing if one is present and throw error for invalid args, and suggest to use the scope one

            Then again this assumes Im implementing the derive stuff

    println("{}", age);

    println("{}", student.name);
}
```

### Loops
```
For Loops:

    Went the range route

    Declaration:
        for (<var> in <range>) {
            ...
        }

        range = start..end
        exclusive end

        range = start..=end;
        inclusive end

        implicitly determine rev or fwd
        if start > end; decrement


    Example:

        for (i in 0..10) {
            ...
        }

        range 0 through 10 exclusive, incrementing

        NOTE:
            the range you specify 
            is exclusive unless 
            ..= is present

            for reversed ranges, we will simply use our brains
            if start < end, obviously you wanted to decrement

            for (i in 10..=0) {
                ...
            }

            range 10 through 0 inclusive, decrementing

        under hood repr (not exposed to user):

            struct Range<i32> {
                start: i32;
                end: i32;
                is_inclusive: boolean = false;
                is_done: boolean = false;
            };

            let range: Range = |Range|; // Make a hallocd (heap allocated) copy of Range struct

            fn next(&range) -> i32? {
                if (range.is_done) {
                    return None;
                }

                if (range.start == range.end) {
                    if (range.is_inclusive) {
                        range.is_done = true;

                        return range.start;
                    }
                    else {
                        range.is_done = true;

                        return None;
                    }
                }

                let current: i32 = range.start;

                if (range.start < range.end) {
                    range.start += 1;
                } 
                else {
                    range.start -= 1;
                }

                return current;
            }
 
For Each Loops:

    Declaration:
        forEach (<element> in <iterable>) {
            ...
        }
    
    Example:
        const characters: [char, 5] = { ... };
        forEach (character in characters) {
            ...
        }

        character becomes of the same type as the individual elements of the characters array

        probably desugars to a regular for loop
        dk how that would look

        thinking to have separate forEach 
        so that i dont have the complexity 
        associated with parsing 'for x in 0..10' 
        and 'for letter in letters'
        basically separating item in collection vs element in iterable

While Loops:

    Declaration:
        while (<condition>) {
            ...
        }
    
    Example:
        while (true) {
            ...
        }

        let x: i64 = 5;
        while (x < 10) {
            ...
            x++;
        }
```