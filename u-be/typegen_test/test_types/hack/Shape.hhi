<?hh
/**
 * Complex enum with different variant types
 */
type Shape = shape(
  ?'Circle' => ?float,
  ?'Rectangle' => ?(float, float),
  ?'Point' => ?shape(
    'x' => float,
    'y' => float,
    'z' => float,
  ),
);
