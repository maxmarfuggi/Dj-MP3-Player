export interface Point {
  x: number;
  y: number;
}

export interface TonearmGeometry {
  /** Where the arm pivots. */
  pivot: Point;
  /** The record's spindle. */
  center: Point;
  /** The stylus tip relative to the pivot, with the arm drawn at 0° (pointing down). */
  stylus: Point;
}

/**
 * The clockwise rotation of the arm, in degrees, that puts the stylus `radius`
 * away from the spindle, on the side of the record the arm swings in from.
 */
export function armAngleForRadius({ pivot, center, stylus }: TonearmGeometry, radius: number): number {
  const toCenter = { x: center.x - pivot.x, y: center.y - pivot.y };
  const pivotToCenter = Math.hypot(toCenter.x, toCenter.y);
  const pivotToStylus = Math.hypot(stylus.x, stylus.y);
  // Rotations that would aim a point straight below the pivot, and the stylus, at the spindle.
  const aimAtCenter = Math.atan2(-toCenter.x, toCenter.y);
  const stylusOffset = Math.atan2(-stylus.x, stylus.y);
  // How far short of the spindle the stylus must stop: the law of cosines in
  // the triangle pivot, spindle, stylus.
  const shortOfCenter = Math.acos(
    (pivotToCenter ** 2 + pivotToStylus ** 2 - radius ** 2) / (2 * pivotToCenter * pivotToStylus),
  );
  return ((aimAtCenter - stylusOffset - shortOfCenter) * 180) / Math.PI;
}
