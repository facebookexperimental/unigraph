<?hh
/**
 * Complex enum with different variant types
 */
type Shape = shape('type' => 'Circle', 'data' => float) | shape('type' => 'Rectangle', 'data' => ('0' => float, '1' => float)) | shape('type' => 'Point', 'x' => float, 'y' => float, 'z' => float);
