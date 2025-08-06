// Test struct with optional fields
type User = shape(
  'id' => int,
  'email' => string,
  'profile' => ?string,
  'verified' => bool,
  'tags' => dict<string, string>,
  'metadata' => dict<string, bool>,
);
