# JS-Magi
An unminifier that does more complicated transformations to make the code readable.  
Note that this does not currently have major transformations, but it does generally make it easier.  
As well, it is not made to avoid problems with adversarial javascript at the current time.  

## Installation
Currently, clone the repo and `cargo build --release`.  
  
TODO: Separate binary.
TODO: Webpage.

## Usage
`js-magi transform ./input.js --output ./output.ts`  
By default it turns the file into Typescript, due to being easier to analyze in typical code editors. This would allow you to specify types, and VSCode appears to provide better type inference for TS files.  

## Transformations
#### Sequence Expander
**Kind**: Minor, Readability  
Converts `a, b, c` into `a; b; c;`.  
This typically makes the code more readable.  

### Void to Undefined
**Kind**: Minor, Readability, Unminification  
Converts `void 0` into `undefined`.   
This is a common minification technique which is rarely used by actual developers, and is generally more readable as `undefined`.

### Not Literal
**Kind**: Minor, Readability, Unminification  
Converts `!0` into `true` and `!(number here)` into `false`.

### Not IIFE
**Kind**: Minor, Readability, Unminification  
Converts `!function(){/*blah*/}()` into `(function(){/*blah*/})()`, when it used as a statement. This is just a trick by minifiers to avoid using one extra parentheses.  

### Init Assignment
**Kind**: Minor, Readability, Unminification  
Converts `(c = n || (n = {})).thing = 'hi'` into  
```js
n = n || {};
c = n;
c.thing = 'hi';
```
Which is more readable and allows future passes to remove unused variable redeclarations.
(TODO: There may be edge cases here. Proxies?)

### IIFE Expand
**Kind**: Medium, Readability, Unminification  
This pass tries to expand basic IIFEs into their body.  
This is useful on code which overuses them (probably to make it easier to minimize?).  
Ex:
```js
(function (e) {
    e.thing = 'hi';
})(l);
//
l.thing = 'hi'
```

```js
(function (e) {
    e.thing = 'hi';
})(l || (l = {}));
//
l = l || {};
l.thing = 'hi';
```

```js
(function (e) {
    e.thing = 'hi';
})(a = l || (l = {}));
//
l = l || {};
a = l;
// This uses the `l` variable instead because it doesn't actually need to use `a` at all.
// and it makes it easier for a later pass to remove the unused variable.
l.thing = 'hi';
```
(TODO: does this last one have issues if `l` is a string?)  

It isn't as fully featured as I'd like at the moment, since it is focusing on expanding for member expressions and single parameters.  
However it has the basic setup to allow me to expand more complicated IIFEs.

### ES Module Rename
If a variable has `Object.defineProperty(j, '__esModule', {..})` on it, then we assume it is an ES module and rename `j` to `exports` to make it clearer.

### Nested Assignment
**Kind**: Minor, Readability  
Converts `a = b = c = ... = 0` into `a = 0; b = 0; c = 0; ...`.    
This isn't always more readable, but it can be. This is also easier for future passes to removed unused variable declarations, or to collapse the future assignments into one.

### Var Decl Expand
**Kind**: Minor, Readability  
Converts `var a = 0, b = 1, c = 2` into `var a = 0; var b = 1; var c = 2`.
This isn't always more readable, but it can be. This is also easier for future passes to removed unused variable declarations, or to collapse the future assignments into one.

### Enum Recognition
Recognizes enum definitions in JavaScript and converts them into TypeScript enums.  
```js
(function (e) {
    e[e.A = 0] = "A";
    e[e.B = 1] = "B";
    e[e.C = 2] = "C";
})(p = exports.Thing || (exports.Thing = {}));
```
into
```ts
exports.Thing = exports.Thing || {};
p = exports.Thing;
enum Thing {
    A = 0,
    B = 1,
    C = 2,
}
Object.assign(exports.Thing, Thing);
```
Not elegant, but the other pieces can hopefully be cleaned up by other passes.


## Ideas
### Painful conditions
stuff like ` this.a || (this.a = !0, this.b && (this.b.fire(void 0), this.dispose()))`
`s && (a = s.type, c = s.handler);`
`return this._token || (this._token = new c), this._token;`

### Source Maps
TODO: might be able to use source maps for some better results?
Some websites provide them.

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

### ES Module Renaming
We could just have a transformation that detects `42: (e, t, n)=>{` and just renames all the variables.

### Basic Define Property
```js
Object.defineProperty(exports, "Thing", {
    enumerable: true,
    get: function() {
        return l.Thing;
    }
});
```
We can probably replace this with `exports.Thing = l.Thing;`.  
A question is whether we can do it in general. Most likely not, because `l.Thing` could be a getter or some other weird thing like that.  
However, it would probably be a fine 'possible non runnable' transformation.

### Class Naming
`exports.Thing = class {`
Could be turned into
`exports.Thing = class Thing {` if we can detect that no one uses the variable `Thing`?  
This would be a bit tricky, and it primarily just gives us the ability to get its name when running the code, so I don't think it is worth the time investment atm.

### Detect Safe Variables
It would be good to have a function which annotates variables in a scope as 'safe to access' or 'unsafe to access' or 'unsure'.  
Some transformation are risky, like field accesses, because they could be getters/setters/proxies.  
However, there is lots of cases where we know the definition of the variable and thus we can know it is safe to access and modify.  
Ex:
```js
exports.Thing = undefined;
// ...
exports.Thing = exports.Thing || {};
let tmp = exports.Thing;
tmp.A = "A";
tmp.B = "B";
```
We can detect that `exports.Thing` is safe to access, and thus we can get rid of `tmp`.  
Though, this could be expensive to do. Especially since to be safe we may need to do it repeatedly due to passes messing with the code.

### Initializers
Sometimes the code, or my generated code, has the form:
```js
var a = undefined;
// ...
a = a || {};
a.blah = "hi";
// ...
a = a || {};
a.thing = "hi";
```
With actions which shouldn't be able to modify it in-between. We should be able to remove the second `a = a || {};` line.  
As well, we could potentially move the `a = a || {};` to where it is initialized.
Then we could do the same thing with the field assignments and just end up with:  
```js
var a = {
    blah: "hi",
    thing: "hi",
};
```
and thus simplify the code quite a bit.

### JSX Conversion
It might be desirable to be able to convert transpiled JSX back into JSX?

### Improve enum gen
Currently we generate enums like:
```typescript
exports.Trace = exports.Trace || {};
p = exports.Trace;
enum Trace {
    Off = 0,
    Messages = 1,
    Verbose = 2
}
Object.assign(exports.Trace, Trace);
```
Due to the way `exports.Trace` is initialized we can't do something smarter like `exports.Trace = Trace;`.  
But! It seems like some generated code does:
```typescript
exports.Trace = undefined;
```
at the start of their module. We could detect this, check that nothing else assigns to it, and then do the nicer version.
```typescript
enum Trace {
    Off = 0,
    Messages = 1,
    Verbose = 2
}
exports.Trace = Trace;
p = exports.Trace;
```
and potentially just get rid of the undefined declaration.

## Known Bugs
- Renaming `(e, t, n)` to `(module, exports, require)` can instead end up with `(module1, exports, require1)` and the like. This is an SWC bug.
- comment on line before root iife can end up just before it after transformation
  - `/* abc */ (() => {})`
- Only sortof bug, but maybe need better default tsconfig: Classes complain that fields aren't defined. Inferring the names of fields probably wouldn't be hard, but the issue is getting the types. We could just use `any`.
- We don't handle typescript types and the like on inputs. This is fine for the most part, since you rarely have typescript code to deobfuscate.

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
- Version which allows you to apply the transformations manually, one at a time.
 - With an editor extension
 - Would allow edits which are hard to detect properly, but that a human might be able to recognize the applicability of
- Have a way of mapping an original span to the span of the resulting file, to make it easier to see what happened.
- Have so some optional destructive transformations leave a comment with a code indicating what transformation was done.
  - like we have `Object.defineProperty` that might be slightly different if you just declare the field normally (because it might disallow setting or something), but a destructive transform would make it more readable. Just leave a code like `// MCode(D001)` or something.
  - and then have a command to get the information about the code. Like how rustc does with `rustc --explain E001`
- Use chatgpt to provide better names for functions and variables. Could also be used to provide comments.
  - Could also be used to simplify code, but that is riskier.