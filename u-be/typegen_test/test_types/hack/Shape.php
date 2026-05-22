// Complex enum with different variant types
type Shape = shape(
  // Circle with radius
  ?'Circle' => ?float,
  // Rectangle with width and height
  ?'Rectangle' => ?(float, float),
  // Point with coordinates
  ?'Point' => ?shape(
    'x' => float,
    'y' => float,
    'z' => float,
  ),
);

enum ShapeVariant: string as string {
  CIRCLE = "Circle";
  RECTANGLE = "Rectangle";
  POINT = "Point";
}
