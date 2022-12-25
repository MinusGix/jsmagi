# JS-Magi
An unminifier that does more complicated transformations to make the code readable.

## Transformations
- Minor: Variable expansion
 - Transform `var n, a, b, c;` into separate variable declarations
 - This is sometimes nice for readability
- TODO: Medium: For isolated function definitions with one-letter variable names, replace them with longer more distinctive variable names.
 - This makes it way easier to avoid confusing them variables outside of the scope
 - As well as making it easier to rename them

## Ideas
### Painful conditions
stuff like ` this.a || (this.a = !0, this.b && (this.b.fire(void 0), this.dispose()))`
`s && (a = s.type, c = s.handler);`
`return this._token || (this._token = new c), this._token;`

### IIFEs
simplifying this to a variable and a separate function call would be nice: `! function (e) {...}(s = t.a || (t.a = {}));`
```js
!function(e1) {
                e1.type = new i.Abc("wow");
            }(l || (l = {}));
            (function(e1) {
                e1.type = new i.Abs("doom");
            })(u || (u = {}));
```
the `!` is just a short way of making the function its own expr so it can be immediately invoked

recognize typescript enum defs
```js
  (function(e1) {
                e1[e1.A = 0] = "A";
                e1[e1.B = 1] = "B";
                e1[e1.C = 2] = "C";
            })(p = t.Thing || (t.Thing = {}));
```
and maybe insert comments specifying that it is a ts enum

For functions like these we can expand them out, to just use the variable directly.

### Source Maps
TODO: might be able to use source maps for some better results?

### Weird Argument order
Sometimes you see `undefined !== v` or `a.thing !== 0` or `"object" == typeof e1`, which is an unnatural ordering (I think so, at least?). We could try to detect this and swap the arguments around.

### Weird Ifs
`if (c) if (undefined === e1.params) {` aaaa
`else k && (d = k(e1.method, e1.params, u.token));` aaaa

### Typescript code generation
We could have a typescript pass which tries transforming various things into typescript code (like the enums) and also tries inserting types for things which are at least obvious.
There is some issues in generating types, though. It would be complex in some cases, especially for areas where there's only partial fields.

### More Void
```js
if (undefined !== a && a.a === i.b) return void r(new Blah(Thing, `blah`), m, l);
```

### Nested Ternaries
One level of ternaries are probably fine, but multiple levels are a good way to be evil. We could try to detect this and expand them out.

### Comments about properties
#### Side-Effect Free
We could mark functions as 'side-effect free'.  
We would probably also want a separate mode which assumes that certain safe browser functions are side-effect free, like `String(x)` or `Array.isArray(x)` or blah

### Common Functions
We could have 'standard names' for common functions / function wrappers / etc. This is weaker than being able to recognize an arbitrary library, but is easier.
Ex:
```js
function n(e) {
    return typeof e === "string" || e instanceof String;
}
```
could be given the name `isString` or something. Might want an extra letter on that to differentiate it from a version without one of the branches.  
There is the issue of replacing the variable name, though.  
As well, it isn't necessarily eval-safe.

### Eval Checks
Some modifications are not necessarily safe when the script is running arbitrary eval'd code. It would be good to have a check for this.  
As well, it would be best to be able to analyze whether it is actually being used and whether it is being called with a string that we can constant evaluate to get the contents. Though modifying the eval'd code at the same time would be tricky.

### ES Module 'unpacking'
Some files define a big object indexed by numbers which are different small modules which can be loaded.
It would be cool to be able to unpack these as different files, or at least separate them more.  
Then you could define types for the export functions/variables, and give them names.

### JSX Conversion
It might be desirable to be able to convert transpiled JSX back into JSX?

## Wacky Unimplemented Ideas
These are ideas that I'd love to implement, but are significantly more complicated and thus might take a while (if they ever appear)!
- Some form of const evaluation. This would try to let us get around some of the more annoying obfuscation techniques
- Function ''hashing'', which extracts the basic structure of a function (independent of variable naming) and hashes it. Then, you can have it identify the future versions
 - The basic version would be to just add a comment above the functions with some obviously unique id, which lets you match it up with your hand deobfuscated version
 - A more complicated version would automatically extract the variable names from the unobfuscated code you give it
- Library recognition. There's a bunch of common libraries which might be obvious with some basic analysis.
- Type definitions
- Use a language model (ex: copilot) to try to auto interpret some parts of code
 - At a basic level, this could get us comments
 - Could also get variable names for us to use
 - Maybe even do transformations that we don't have implemented, though that is risker