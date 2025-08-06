// Simple address struct for testing
type Address = shape(
  // Street address
  'street' => string,
  'city' => string,
  'zip_code' => int,
  'coordinates' => vec<float>,
  'typegen_as' => int,
  'string_list' => vec<string>,
  'maybe_flag' => ?bool,
);
