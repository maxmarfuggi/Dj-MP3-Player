// Wall player, flat club-style shell.
//
// A rectangular box that hangs on the wall, in the proportion of an XDJ-1000:
// no slope. The face is parallel to the wall. A 7" display sits in the upper
// half, a flat jog platter in the middle, a play/pause key in the bottom-left
// corner, and the USB ports along the top edge.
//
// The shell is meant to be printed clear, so the Pi and the encoder stay
// visible. Nothing on it is labeled.
//
//   Jog platter: browse. Turn to scroll, press to select, hold to go back.
//   Play key:    play or pause. A momentary switch, not an encoder.
//
// Units are millimetres. +x is right, +y is up when the player is on the
// wall, +z comes out of the wall toward the room. Open this file in OpenSCAD,
// leave `part` on "assembly", and press F5. To export a part, set `part` and
// press F6, then File > Export > STL:
//
//   openscad -o body_lower.stl -D 'part="body_lower"' shell.scad
//   openscad -o body_upper.stl -D 'part="body_upper"' shell.scad
//   openscad -o face_lower.stl -D 'part="face_lower"' shell.scad
//   openscad -o face_upper.stl -D 'part="face_upper"' shell.scad
//   openscad -o jog.stl        -D 'part="jog"'        shell.scad
//   openscad -o play.stl       -D 'part="play"'       shell.scad
//   openscad -o splice.stl     -D 'part="splice"'     shell.scad   (print 2)
//   openscad -o pin.stl        -D 'part="pin"'        shell.scad   (print 2)
//
// Every exported part already sits on the bed. None of them need supports.
// The halves are split so each one fits a 250 × 210 mm bed.

part = "assembly"; // [assembly, exploded, section, body_lower, body_upper, face_lower, face_upper, jog, play, splice, pin]

/* [Shell] */
case_w = 236;     // width
case_h = 362;     // height on the wall
case_z = 52;      // depth off the wall, constant
wall = 3.4;
rim_t = 1.6;      // lip that wraps the face
floor_t = 4.8;    // skin against the wall; thick enough for the keyholes
deck_t = 4.0;     // face thickness
corner_r = 0;     // outside corners; 0 is a hard corner
fit = 0.45;
split_y = 194;    // seam between the platter and the display
seam = 0.25;

/* [Display: Raspberry Pi Touch Display 2, 7 inch] */
disp_w = 189.5;
disp_h = 120.2;
disp_t = 8.6;
disp_r = 0;       // screen corners; 0 is a hard corner
window_lip = 3.5;
screen_y0 = 208;  // bottom edge of the display, measured up from the floor

/* [Jog platter] */
jog_d = 150;
jog_x = 118;
jog_y = 108;
jog_h = 6.6;      // overall, and the top face is flat
well_drop = 2.2;  // recess the platter sits in
center_d = 32;    // flat center pad
center_raise = 0.55;

// EC11 / KY-040: M7 bushing, 6 mm D-shaft.
enc_hole_d = 7.6;
panel_t = 1.6;    // plastic left under the encoder nut
shaft_d = 6;
shaft_flat = 4.5;
shaft_depth = 5.2;
nut_d = 14;

/* [Play / pause key, bottom left] */
play_x = 16;
play_y = 16;
play_d = 36;      // circular cap, CDJ play key
play_t = 5.4;     // cap thickness; the top is flat
play_well = 1.15; // recess in the face
play_stem_d = 7.4;
play_hole_d = 9.2;

/* [USB pocket, top left, above the display] */
usb_x = 14;
usb_y = 336;
usb_w = 100;
usb_h = 16;
// [center along the pocket, cutout width, cutout height, screw spacing]
usb_a = [28, 15.5, 7.6, 30];
usb_c = [70, 10.5, 4.4, 0];

/* [Bottom edge] */
audio_jack_d = 6.4; // set to 0 to leave the jack out
audio_jack_x = 188;

/* [Screws] */
pilot_d = 2.5;
clear_d = 3.4;
boss_d = 12;

$fn = 64;

// ----------------------------------------------------------------- derived

inset = rim_t + fit;
face_w = case_w - 2 * inset;
face_h = case_h - 2 * inset;
// A sharp outside stays sharp through the wall, the rim, and the face.
// A rounded outside keeps a small fillet after each inset.
function inset_r(outer, cut) = outer <= 0 ? 0 : max(0.8, outer - cut);
r_wall = inset_r(corner_r, wall);
r_rim = inset_r(corner_r, rim_t);
r_face = inset_r(corner_r, inset);
screen_r = disp_r <= 0 ? 0 : disp_r;
well_r = jog_d / 2 + 2.2;
screen_cx = case_w / 2;
screen_cy = screen_y0 + disp_h / 2;

// Posts the face screws into. Lower half is y < split_y.
bosses = [
  [206, 36],
  [18, 96],
  [218, 156],
  [18, 156],
  [15, 252],
  [221, 252],
  [15, 312],
  [221, 312],
];

// Big hole below, narrow slot above. The case slides down onto the screws.
keyholes = [
  [case_w / 2 - 60, case_h - 30],
  [case_w / 2 + 60, case_h - 30],
];

splices = [
  [58, split_y],
  [176, split_y],
];
splice_w = 18;
splice_l = 32;
splice_t = 4;
pad_h = 5;

function face_x(x) = x - inset;
function face_y(y) = y - inset;

echo("screen top", screen_y0 + disp_h);
echo("jog well top", jog_y + well_r);
echo("usb top", usb_y + usb_h);
echo("face size", face_w, face_h);
echo("lower piece height", split_y);

// ----------------------------------------------------------------- primitives

module rounded_rect(w, h, r) {
  if (r <= 0.05)
    square([w, h]);
  else {
    rr = min(r, min(w, h) / 2 - 0.05);
    offset(r = rr) offset(delta = -rr) square([w, h]);
  }
}

module rounded_rect_c(w, h, r) {
  translate([-w / 2, -h / 2]) rounded_rect(w, h, r);
}

module d_hole(depth) {
  intersection() {
    cylinder(d = shaft_d + 0.25, h = depth + 2);
    translate([-shaft_d, -shaft_d, -1])
      cube([shaft_d / 2 + shaft_flat + 0.15, 2 * shaft_d, depth + 4]);
  }
}

module wall_ring(thickness, radius) {
  difference() {
    linear_extrude(case_z + 2) rounded_rect(case_w, case_h, corner_r);
    translate([thickness, thickness, -1])
      linear_extrude(case_z + 4)
        rounded_rect(case_w - 2 * thickness, case_h - 2 * thickness, radius);
  }
}

// ----------------------------------------------------------------- face
// Outer face at z = deck_t. This is also the print orientation.

module face_flat() {
  difference() {
    linear_extrude(deck_t) rounded_rect(face_w, face_h, r_face);
    screen_cut();
    jog_cuts();
    play_cuts();
    usb_cuts();
    face_screw_holes();
    pin_holes();
  }
}

module screen_cut() {
  ow = disp_w - 2 * window_lip;
  oh = disp_h - 2 * window_lip;
  translate([face_x(screen_cx), face_y(screen_cy), 0]) {
    translate([0, 0, -2])
      linear_extrude(deck_t + 4) rounded_rect_c(ow, oh, screen_r);
    hull() {
      translate([0, 0, deck_t - 1.4])
        linear_extrude(0.2) rounded_rect_c(ow, oh, screen_r);
      translate([0, 0, deck_t - 0.15])
        linear_extrude(0.2) rounded_rect_c(ow + 2.2, oh + 2.2, screen_r);
    }
  }
}

module jog_cuts() {
  translate([face_x(jog_x), face_y(jog_y), 0]) {
    translate([0, 0, deck_t - well_drop])
      cylinder(d = well_r * 2, h = well_drop + 1);
    translate([0, 0, panel_t]) cylinder(d = nut_d, h = deck_t);
    translate([0, 0, -1]) cylinder(d = enc_hole_d, h = deck_t + 2);
    // One machined ring just outside the well.
    gr = well_r + 2.6;
    translate([0, 0, deck_t - 0.4])
      difference() {
        cylinder(d = gr * 2 + 1.2, h = 1);
        translate([0, 0, -0.2]) cylinder(d = gr * 2 - 1.2, h = 1.4);
      }
  }
}

module play_cuts() {
  translate([
    face_x(play_x + play_d / 2),
    face_y(play_y + play_d / 2),
    deck_t - play_well
  ]) cylinder(d = play_d + 2.4, h = play_well + 1);
  translate([
    face_x(play_x + play_d / 2),
    face_y(play_y + play_d / 2),
    -1
  ]) cylinder(d = play_hole_d, h = deck_t + 2);
}

module usb_cuts() {
  translate([face_x(usb_x), face_y(usb_y), deck_t - 1.6])
    linear_extrude(2) rounded_rect(usb_w, usb_h, 2);
  for (p = [usb_a, usb_c]) {
    translate([
      face_x(usb_x + p[0] - p[1] / 2),
      face_y(usb_y + usb_h / 2 - p[2] / 2),
      -1
    ]) linear_extrude(deck_t + 2) rounded_rect(p[1], p[2], 0.8);
    if (p[3] > 0)
      for (s = [-1, 1])
        translate([
          face_x(usb_x + p[0] + s * p[3] / 2),
          face_y(usb_y + usb_h / 2),
          -1
        ]) cylinder(d = clear_d, h = deck_t + 2);
  }
}

module face_screw_holes() {
  for (b = bosses)
    translate([face_x(b[0]), face_y(b[1]), 0]) {
      translate([0, 0, -1]) cylinder(d = clear_d, h = deck_t + 2);
      translate([0, 0, deck_t - 1.6])
        cylinder(d1 = clear_d, d2 = 6.6, h = 1.7);
    }
}

module pin_holes() {
  s = face_y(split_y);
  for (x = [72, face_w - 72])
    translate([x, s, deck_t / 2])
      rotate([-90, 0, 0])
        cylinder(d = 2.2, h = 14, center = true);
}

module face_lower() {
  s = face_y(split_y);
  intersection() {
    face_flat();
    translate([-2, -2, -1]) cube([face_w + 4, s + 2, deck_t + 2]);
  }
}

module face_upper() {
  s = face_y(split_y);
  intersection() {
    face_flat();
    translate([-2, s, -1]) cube([face_w + 4, face_h + 4, deck_t + 2]);
  }
}

// ----------------------------------------------------------------- body
// z = 0 is the wall side. Printed wall-side down.

module keyhole(x, y) {
  translate([x, y, -1]) cylinder(d = 9.2, h = floor_t + 2);
  translate([x - 2.2, y, -1]) cube([4.4, 11, floor_t + 2]);
  translate([x, y + 11, -1]) cylinder(d = 4.4, h = floor_t + 2);
  // Head pocket open to the inside, leaving 2 mm against the wall.
  translate([x - 5.5, y - 5, 2]) cube([11, 22, floor_t]);
}

module body_shell() {
  difference() {
    linear_extrude(case_z) rounded_rect(case_w, case_h, corner_r);
    // Interior, above the floor.
    translate([wall, wall, floor_t])
      linear_extrude(case_z)
        rounded_rect(case_w - 2 * wall, case_h - 2 * wall, r_wall);
    // The face drops into this opening and lands on the shelf.
    translate([rim_t, rim_t, case_z - deck_t])
      linear_extrude(deck_t + 2)
        rounded_rect(case_w - 2 * rim_t, case_h - 2 * rim_t, r_rim);
  }
}

module boss_at(x, y) {
  translate([x, y, floor_t - 0.2])
    cylinder(d = boss_d, h = case_z - deck_t - floor_t + 0.2);
}

module display_posts() {
  x0 = (case_w - disp_w) / 2;
  y0 = screen_y0;
  pts = [
    [x0 + 14, y0 + 14],
    [x0 + disp_w - 14, y0 + 14],
    [x0 + 14, y0 + disp_h - 14],
    [x0 + disp_w - 14, y0 + disp_h - 14],
  ];
  h = case_z - deck_t - disp_t - 0.7;
  for (p = pts)
    translate([p[0], p[1], floor_t - 0.2])
      cylinder(d = 8, h = h - floor_t + 0.2);
}

module splice_pads() {
  for (s = splices)
    translate([s[0] - splice_w / 2, s[1] - splice_l / 2, floor_t])
      cube([splice_w, splice_l, pad_h]);
}

module edge_features() {
  // Power lead, out the bottom edge.
  translate([case_w / 2 - 7, -1, 14])
    cube([14, wall + 2, 8]);
  for (dx = [-18, 18])
    translate([case_w / 2 + dx, -1, 18])
      rotate([-90, 0, 0])
        cylinder(d = 4.2, h = wall + 2);
  if (audio_jack_d > 0)
    translate([audio_jack_x, -1, 24])
      rotate([-90, 0, 0])
        cylinder(d = audio_jack_d + 0.3, h = wall + 2);
  // Intake along the bottom, exhaust up the sides near the Pi.
  for (i = [0:4])
    translate([36 + i * 28, -1, 30])
      cube([14, wall + 2, 2.4]);
  for (side = [0, 1]) {
    x0 = side == 0 ? -1 : case_w - wall - 0.2;
    for (i = [0:3])
      translate([x0, 250 + i * 16, 20 + (i % 2) * 8])
        cube([wall + 2, 12, 2.4]);
  }
}

module body_full() {
  difference() {
    union() {
      body_shell();
      for (b = bosses) boss_at(b[0], b[1]);
      display_posts();
      splice_pads();
    }
    for (b = bosses)
      translate([b[0], b[1], case_z - deck_t - 10])
        cylinder(d = pilot_d, h = 12);
    for (s = splices)
      for (dy = [-8, 8])
        translate([s[0], s[1] + dy, floor_t - 1])
          cylinder(d = pilot_d, h = pad_h + 2);
    for (k = keyholes) keyhole(k[0], k[1]);
    edge_features();
  }
}

module body_lower() {
  intersection() {
    body_full();
    translate([-4, -4, -4]) cube([case_w + 8, split_y + 4, case_z + 8]);
  }
}

module body_upper() {
  intersection() {
    body_full();
    translate([-4, split_y + seam, -4])
      cube([case_w + 8, case_h, case_z + 8]);
  }
}

// ----------------------------------------------------------------- small parts

// Flat platter. The top is one plane; the grip is only on the outer edge,
// and a hairline ring separates that edge from the playing surface.
module jog() {
  difference() {
    union() {
      cylinder(d = jog_d - 0.4, h = jog_h);
      cylinder(d = center_d, h = jog_h + center_raise);
      for (i = [0:95])
        rotate(i * 360 / 96)
          translate([jog_d / 2 - 0.55, 0, 0])
            cylinder(d = 1.15, h = jog_h, $fn = 6);
    }
    translate([0, 0, -1]) d_hole(shaft_depth);
    // Channel between the grip band and the flat playing surface.
    translate([0, 0, jog_h - 0.75])
      difference() {
        cylinder(d = jog_d - 13, h = 1.2);
        translate([0, 0, -0.2]) cylinder(d = jog_d - 18.2, h = 1.6);
      }
    // Short radial ticks on the grip band, sunk into the flat top.
    for (i = [0:79])
      rotate(i * 4.5)
        translate([jog_d / 2 - 9.2, -0.35, jog_h - 0.5])
          cube([7.6, 0.7, 0.8]);
    // Spindle dimple. Does not break through the top.
    translate([0, 0, jog_h + center_raise - 0.4])
      cylinder(d = 3.4, h = 1);
  }
}

// Circular play/pause key. Flat top, triangle sunk into it.
// The stem passes through the face and lands on a momentary switch.
module play() {
  difference() {
    union() {
      translate([play_d / 2, play_d / 2, 0])
        cylinder(d = play_d, h = play_t);
      translate([play_d / 2, play_d / 2, -7.5])
        cylinder(d = play_stem_d, h = 7.5);
    }
    translate([play_d / 2 - 6.2, play_d / 2, play_t - 2.1])
      linear_extrude(2.4)
        polygon([[0, -7], [0, 7], [12.4, 0]]);
  }
}

module splice() {
  difference() {
    linear_extrude(splice_t) rounded_rect(splice_w, splice_l, 2);
    for (dy = [-8, 8])
      translate([splice_w / 2, splice_l / 2 + dy, -1])
        cylinder(d = clear_d, h = splice_t + 2);
  }
}

module pin() {
  cylinder(d = 1.85, h = 12, $fn = 16);
}

// ------------------------------------------------------- preview internals

module guts() {
  color([0.015, 0.016, 0.02])
    translate([face_x(screen_cx), face_y(screen_cy), -disp_t])
      linear_extrude(disp_t)
        rounded_rect_c(disp_w, disp_h, screen_r);
  translate([face_x(screen_cx), face_y(screen_cy) - 2, -(disp_t + 8)]) {
    color([0.07, 0.32, 0.14]) cube([85, 56, 1.6], center = true);
    color([0.55, 0.58, 0.6]) translate([0, 0, -6]) cube([36, 36, 8], center = true);
  }
  translate([face_x(jog_x), face_y(jog_y), -14]) {
    color([0.08, 0.34, 0.16]) cube([19, 19, 12], center = true);
    color([0.72, 0.74, 0.76]) translate([0, 0, 6]) cylinder(d = 7, h = 14);
  }
}

module screen_glass() {
  ow = disp_w - 2 * window_lip - 0.8;
  oh = disp_h - 2 * window_lip - 0.8;
  translate([face_x(screen_cx) - ow / 2, face_y(screen_cy) - oh / 2, deck_t - 0.5])
    linear_extrude(0.4) rounded_rect(ow, oh, screen_r);
}

// ----------------------------------------------------------------- views

shell_c = [0.80, 0.86, 0.90, 0.24];
jog_c = [0.90, 0.93, 0.95, 0.45];
play_c = [0.18, 0.19, 0.20, 0.92];
metal_c = [0.62, 0.64, 0.66, 1];

module place_face() {
  translate([inset, inset, case_z - deck_t]) children();
}

module assembly() {
  color(shell_c) body_full();
  color(shell_c) place_face() face_flat();
  if ($preview) {
    color([0.01, 0.012, 0.016]) place_face() screen_glass();
    place_face() guts();
  }
  // Platter nested in the well, top parallel to the wall.
  color(jog_c)
    translate([jog_x, jog_y, case_z - well_drop + 0.35])
      jog();
  color(play_c)
    translate([play_x, play_y, case_z - play_well + 0.25])
      play();
  color(metal_c)
    for (s = splices)
      translate([s[0] - splice_w / 2, s[1] - splice_l / 2, floor_t + pad_h])
        splice();
}

module exploded() {
  color(shell_c) body_full();
  translate([0, 0, 36]) {
    color(shell_c) place_face() face_flat();
    if ($preview) color([0.01, 0.012, 0.016]) place_face() screen_glass();
    color(jog_c)
      translate([jog_x, jog_y, case_z - well_drop + 18])
        jog();
    color(play_c)
      translate([play_x, play_y, case_z - play_well + 16])
        play();
  }
}

module section_view() {
  difference() {
    assembly();
    translate([jog_x, -20, -20]) cube([400, case_h + 40, case_z + 40]);
  }
}

// ----------------------------------------------------------------- dispatch

if (part == "assembly") assembly();
else if (part == "exploded") exploded();
else if (part == "section") section_view();
else if (part == "body_lower") body_lower();
else if (part == "body_upper")
  translate([0, -(split_y + seam), 0]) body_upper();
else if (part == "face_lower") face_lower();
else if (part == "face_upper")
  translate([0, -face_y(split_y), 0]) face_upper();
else if (part == "jog") jog();
else if (part == "play")
  // Icon face on the bed, stem up. The triangle is a small bridge on the first layers.
  translate([0, 0, play_t]) rotate([180, 0, 0]) play();
else if (part == "splice") splice();
else if (part == "pin") pin();
