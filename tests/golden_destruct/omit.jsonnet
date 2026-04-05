// ...rest works like removeField, but freezes the object core so it still can access the fields
local {a, b, ...c} = {a: 1, b: 2, c: self.a, d: self.b}; c
