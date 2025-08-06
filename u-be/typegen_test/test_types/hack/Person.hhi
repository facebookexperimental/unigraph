<?hh
/**
 * Person struct that references Address
 */
type Person = shape(
  'name' => string,
  'age' => int,
  'address' => Address,
);
