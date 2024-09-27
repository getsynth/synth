Synth's `number` type allows for generating fixed-width numbers.

### Parameters

#### `subtype`

All the variants of `number` accept an optional `"subtype"` field to specify
the width and primitive kind of the values generated. The value of `"subtype"`,
if specified, must be one of `u64`, `i64`, `f64`, `u32`, `i32`, `f32`, `i8`.

#### Example

```json synth
{
  "type": "number",
  "subtype": "u32",
  "constant": 42
}
```

It is not required to specify the `"subtype"` field: `synth` will try to infer
the best value based on the value of other parameters. But it may be necessary
to set it manually in situations where the data sink only accepts certain
widths (e.g. postgres).

### Defaults

If no variant (such as `range`, `constant`, etc.) is specified, `number`
will have different default behavior based on the value of `"subtype"`.

- For integer subtypes (`i32`, `u32`, etc.): `number` will default to generating one of the integers in the
  representable range of the subtype.
- For float subtypes (`f32`, `f64`): `number` will default to generating from the semi-open interval `[0, 1)`.

#### Example

```json synth
{
  "type": "number",
  "subtype": "i32"
}
```

#### Example

```json synth
{
  "type": "number",
  "subtype": "f32"
}
```

## range

Generates numbers of a particular type contained in a specified interval.

### Parameters

- `"low"` (optional, number): the lower bound of the interval
- `"high"` (optional, number): the upper bound of the interval
- `"step"` (optional, number): force alignment of generated numbers on multiples
  of `"step"` from the value of `"low"`.
- `"include_low"` (optional, bool): whether to include the specified lower bound
  in the range. Defaults to `true`.
- `"include_high"` (optional, bool): whether to include the specified upper
  bound in the range. Defaults to `false`.

#### Example

This generates one of the integers `0, 3, 6, 9`.

```json synth
{
  "type": "number",
  "range": {
      "low": 0,
      "high": 10,
      "step": 3
  }
}
```

#### Example

This generates one integer between `0` (included) and `122` (included).

```json synth
{
  "type": "number",
  "range": {
      "high": 122,         // the age of the oldest recorded person
      "include_high": true      
  }
}
```

#### Example

This generates one floating-point number between `-273.15` (included)
and `15000000.0` (excluded) with an approximate alignment to the second decimal.

```json synth
{
  "type": "number",
  "range": {
      "high": 15000000.0, // temperature at sun's core in Celcius
      "low": -273.15,     // 0 Kelvin
      "step": 0.01
  }
}
```

### Defaults

For values of `"subtype"` belonging to the integer class (`i32`, `u32`, etc.),
the parameters `"low"`, `"high"` and `"step"` default to the following values if
not specified explicitly:

- `"low"`: the minimum representable integer in the subtype
- `"high"`: the maximum representable integer in the subtype
- `"step"`: the integer `1`

#### Example

Not specifying any of `"low"`, `"high"` is equivalent to setting the bounds to
the minimum and maximum representable integers in the subtype.

```json synth
{
  "type": "number",
  "subtype": "i32",
  "range": {}
}
```

For values of `"subtype"` belonging to the float class (`f32`, `f64`, etc.), the
parameters `"low"`, `"high"` default to the following values if not specified
explicitly:

- `"low"`: the floating-point number `0.`
- `"high"`: the floating-point number `1.`

## constant

A constant number type. This will always evaluate to the same number.

#### Example

```json synth
{
  "type": "number",
  "constant": 3.14159  // pi
}
```

The constant number generator can also be simply declared by its desired output value.

#### Example

The schema

```json synth
{
  "type": "object",
  "just_the_number_42": 42
}
```

is the same as the longer

```json synth
{
  "type": "object",
  "just_the_number_42": {
    "type": "number",
    "constant": 42
  }
}
```

## id

A monotonically increasing number type, most commonly used as a unique row identifier. The optional `start` field
defaults to 1 if unspecified.

Synth currently supports `u64` ids.

#### Example

```json synth
{
  "type": "array",
  "length": {
    "type": "number",
    "constant": 5
  },
  "content": {
    "type": "number",
    "id": {
      "start_at": 10
    }
  }
}
```

