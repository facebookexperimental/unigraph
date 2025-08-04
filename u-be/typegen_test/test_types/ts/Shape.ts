/** Complex enum with different variant types */
export type Shape = 
  /** Circle with radius */
  { "Circle": number } | 
  /** Rectangle with width and height */
  { "Rectangle": [number, number] } | 
  /** Point with coordinates */
  { "Point": { x: number, y: number, z: number } };