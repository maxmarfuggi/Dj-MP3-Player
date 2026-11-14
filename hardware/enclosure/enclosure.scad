// DJ USB player, proof-of-concept enclosure.
//
// Etch A Sketch style: a 7" Raspberry Pi Touch Display 2 in landscape, with
// two big knobs in the bottom corners and the USB ports on the right side.
//
// Units are millimetres. Open this file in OpenSCAD (free) to see the
// assembly, then export each part for printing from the command line:
//
//   openscad -o front.stl      -D 'part="front"'      enclosure.scad
//   openscad -o back.stl       -D 'part="back"'       enclosure.scad
//   openscad -o port_plate.stl -D 'part="port_plate"' enclosure.scad
//   openscad -o retainer.stl   -D 'part="retainer"'   enclosure.scad   (print 4)
//   openscad -o knob.stl       -D 'part="knob"'       enclosure.scad   (print 2)
//
// Every part is already oriented for printing without supports.
//
// The screen faces -z, so when you look at the screen, +x runs to the left.
// The ports therefore live on the x = 0 wall, which is the right-hand side.

part = "assembly"; // [assembly, front, back, port_plate, retainer, knob]

/* [Case] */
case_w = 240;
case_h = 200;
case_d = 55;   // total depth, back plate included
wall = 2.5;
back_t = 3;
corner_r = 10;

/* [Display: Raspberry Pi Touch Display 2, 7 inch, turned to landscape] */
disp_w = 189.5;
disp_h = 120.2;
// Thickness at the edges, where the retainers press. Measure your display.
disp_t = 8.6;
disp_r = 8;
disp_top_margin = 22;
// How far the front wall overlaps the glass on each side.
window_lip = 4;
fit = 0.5;

/* [Knobs and encoders] */
knob_inset_x = 34;
knob_y = 30;
// EC11 / KY-040 encoders have an M7 threaded bushing.
enc_hole_d = 7.6;
// The wall is thinned around each encoder so the nut can reach its thread.
enc_panel_t = 1.6;
enc_recess_d = 18;

knob_d = 40;
knob_h = 18;
shaft_d = 6;
// Across the flat of the D-shaped shaft. Measure your encoder.
shaft_flat = 4.5;
shaft_depth = 11;
nut_recess_d = 14;
nut_recess_h = 4;
knob_ridges = 48;

/* [Side ports] */
port_center_y = 118;
port_plate_len = 100;
port_plate_t = 2.5;
// Ports sit behind the display, which fills the front ~11 mm of the case.
port_opening_z0 = 16;

// Cutouts on the port plate. Measure your panel-mount cables and adjust:
// [position along the plate, cutout width, cutout height, screw spacing (0 = none)]
usb_a = [24, 15.5, 8, 30];
usb_c = [56, 10.5, 5, 20];
// Round hole for a panel-mount 3.5 mm jack extension. Set to 0 to leave it out.
audio_jack_u = 84;
audio_jack_d = 6.4;

/* [Screws] */
// M3 screws driven straight into plastic.
pilot_d = 2.6;
clear_d = 3.4;
boss_d = 9;
boss_inset = 7;

$fn = 64;

// Derived positions.
inner_end = case_d - back_t;
disp_x0 = (case_w - disp_w) / 2;
disp_y0 = case_h - disp_top_margin - disp_h;
disp_cy = disp_y0 + disp_h / 2;
knob_xs = [knob_inset_x, case_w - knob_inset_x];
port_y0 = port_center_y - port_plate_len / 2;
port_y1 = port_center_y + port_plate_len / 2;
port_plate_z0 = wall + 0.5;
port_plate_h = inner_end - 0.3 - port_plate_z0;
port_v = (port_opening_z0 + inner_end) / 2 - port_plate_z0;

corner_bosses = [
  [boss_inset, boss_inset],
  [case_w - boss_inset, boss_inset],
  [boss_inset, case_h - boss_inset],
  [case_w - boss_inset, case_h - boss_inset],
];

retainer_offset = fit + 5;
// [x, y, direction the tab points]
retainers = [
  [disp_x0 - retainer_offset, disp_cy - 35, 0],
  [disp_x0 - retainer_offset, disp_cy + 35, 0],
  [disp_x0 + disp_w + retainer_offset, disp_cy - 35, 180],
  [disp_x0 + disp_w + retainer_offset, disp_cy + 35, 180],
];

keyholes = [
  [case_w / 2 - 60, case_h - 45],
  [case_w / 2 + 60, case_h - 45],
];

module rounded_rect(w, h, r) {
  offset(r = r) offset(delta = -r) square([w, h]);
}

module rr_slab(x, y, w, h, r, z0, z1) {
  translate([x, y, z0]) linear_extrude(z1 - z0) rounded_rect(w, h, r);
}

// ---------------------------------------------------------------- front shell

module display_frame() {
  difference() {
    rr_slab(disp_x0 - fit - 1.6, disp_y0 - fit - 1.6,
            disp_w + 2 * fit + 3.2, disp_h + 2 * fit + 3.2,
            disp_r + fit + 1.6, 0, wall + 5);
    rr_slab(disp_x0 - fit, disp_y0 - fit,
            disp_w + 2 * fit, disp_h + 2 * fit,
            disp_r + fit, wall, wall + 6);
  }
}

module window_cut() {
  x = disp_x0 + window_lip;
  y = disp_y0 + window_lip;
  w = disp_w - 2 * window_lip;
  h = disp_h - 2 * window_lip;
  r = disp_r - window_lip;
  chamfer = 1.5;
  hull() {
    rr_slab(x - chamfer, y - chamfer, w + 2 * chamfer, h + 2 * chamfer, r + chamfer, -1, 0);
    rr_slab(x, y, w, h, r, chamfer, wall + 1);
  }
}

// Two guides that hold the port plate flat against the inside of the side
// wall. The plate slides in from the back and the back plate traps it.
module port_guides() {
  slot = port_plate_t + 0.4;
  difference() {
    translate([wall, port_y0 - 3, wall])
      cube([slot + 2, 7, inner_end - wall]);
    translate([wall - 1, port_y0 - 0.2, 0])
      cube([slot + 1, 8, inner_end + 1]);
  }
  difference() {
    translate([wall, port_y1 - 4, wall])
      cube([slot + 2, 7, inner_end - wall]);
    translate([wall - 1, port_y1 - 8 + 0.2, 0])
      cube([slot + 1, 8, inner_end + 1]);
  }
}

// Vent slots run front to back, so their tops only bridge 3 mm when printed.
module vent_slots(xs, y) {
  for (x = xs)
    translate([x - 1.5, y, 18]) cube([3, wall + 2, 26]);
}

module front_shell() {
  difference() {
    union() {
      difference() {
        rr_slab(0, 0, case_w, case_h, corner_r, 0, inner_end);
        rr_slab(wall, wall, case_w - 2 * wall, case_h - 2 * wall,
                corner_r - wall, wall, inner_end + 1);
      }
      display_frame();
      for (p = corner_bosses)
        translate([p[0], p[1], 0]) cylinder(d = boss_d, h = inner_end);
      for (r = retainers)
        translate([r[0], r[1], 0]) cylinder(d = 8, h = wall + disp_t);
      port_guides();
    }

    window_cut();

    for (x = knob_xs) {
      translate([x, knob_y, -1]) cylinder(d = enc_hole_d, h = wall + 2);
      translate([x, knob_y, enc_panel_t]) cylinder(d = enc_recess_d, h = wall);
    }

    for (p = corner_bosses)
      translate([p[0], p[1], inner_end - 14]) cylinder(d = pilot_d, h = 15);
    for (r = retainers)
      translate([r[0], r[1], wall + disp_t - 8]) cylinder(d = pilot_d, h = 9);

    // Opening for the port plate, open towards the back so nothing has to bridge.
    translate([-1, port_y0 + 6, port_opening_z0])
      cube([wall + 1.01, port_plate_len - 12, inner_end]);

    // Notch for the power cable, also open towards the back.
    translate([case_w / 2 - 6, -1, inner_end - 9]) cube([12, wall + 2, 10]);

    vent_slots([for (i = [0:11]) case_w / 2 - 55 + i * 10], case_h - wall - 1);
    vent_slots([70, 80, 90, 100, 140, 150, 160, 170], -1);
  }
}

// ----------------------------------------------------------------- back plate

// Modelled with the wall side at z = 0, which is also how it prints.
module keyhole(k) {
  translate([k[0], k[1], -1]) cylinder(d = 9.5, h = back_t + 4);
  translate([k[0] - 2.25, k[1], -1]) cube([4.5, 10, back_t + 4]);
  translate([k[0], k[1] + 10, -1]) cylinder(d = 4.5, h = back_t + 4);
  // Room for the screw head to slide behind the plate.
  translate([k[0] - 5, k[1] - 5, 2]) cube([10, 20, back_t + 2]);
}

module back_plate() {
  difference() {
    union() {
      rr_slab(0, 0, case_w, case_h, corner_r, 0, back_t);
      for (k = keyholes)
        translate([k[0] - 11, k[1] - 10, 0]) cube([22, 32, back_t + 2]);
    }
    for (p = corner_bosses)
      translate([p[0], p[1], -1]) cylinder(d = clear_d, h = back_t + 2);
    for (k = keyholes) keyhole(k);
    for (i = [-4:4])
      translate([case_w / 2 + i * 8 - 1.5, case_h / 2 - 25, -1]) cube([3, 50, back_t + 2]);
  }
}

// ----------------------------------------------------------------- port plate

module screw_pair(u, v, spacing) {
  if (spacing > 0)
    for (s = [-1, 1])
      translate([u + s * spacing / 2, v]) circle(d = clear_d);
}

// Flat in X/Y: X runs along the case height, Y runs front to back.
module port_plate() {
  linear_extrude(port_plate_t) difference() {
    square([port_plate_len, port_plate_h]);
    for (p = [usb_a, usb_c]) {
      translate([p[0] - p[1] / 2, port_v - p[2] / 2]) rounded_rect(p[1], p[2], 1);
      screw_pair(p[0], port_v, p[3]);
    }
    if (audio_jack_u > 0)
      translate([audio_jack_u, port_v]) circle(d = audio_jack_d);
  }
}

// ---------------------------------------------------------------------- small parts

module retainer() {
  difference() {
    hull() {
      cylinder(d = 10, h = 3);
      translate([9, 0, 0]) cylinder(d = 10, h = 3);
    }
    translate([0, 0, -1]) cylinder(d = clear_d, h = 5);
  }
}

// Modelled with the shaft side at z = 0.
module knob() {
  difference() {
    union() {
      cylinder(d = knob_d - 1.6, h = knob_h);
      for (i = [0:knob_ridges - 1])
        rotate(i * 360 / knob_ridges)
          translate([knob_d / 2 - 1.2, 0, 0]) cylinder(d = 1.6, h = knob_h, $fn = 12);
    }
    translate([0, 0, -1]) intersection() {
      cylinder(d = shaft_d + 0.2, h = shaft_depth + 1);
      translate([-shaft_d, -shaft_d, 0])
        cube([shaft_d + shaft_flat - shaft_d / 2 + 0.1, 2 * shaft_d, shaft_depth + 1]);
    }
    translate([0, 0, -1]) cylinder(d = nut_recess_d, h = nut_recess_h + 1);
    // Soft edge on the face.
    translate([0, 0, knob_h - 1])
      difference() {
        cylinder(d = knob_d + 2, h = 2);
        cylinder(d1 = knob_d, d2 = knob_d - 2, h = 1);
      }
  }
}

// ------------------------------------------------------------------- assembly

module assembly() {
  color("firebrick") front_shell();
  color("#1b1b1f") translate([disp_x0, disp_y0, wall]) cube([disp_w, disp_h, disp_t]);
  color("firebrick", 0.6) translate([0, 0, case_d]) mirror([0, 0, 1]) back_plate();
  color("silver")
    multmatrix([[0, 0, 1, wall],
                [1, 0, 0, port_y0],
                [0, 1, 0, port_plate_z0],
                [0, 0, 0, 1]])
      port_plate();
  for (r = retainers)
    color("gray") translate([r[0], r[1], wall + disp_t]) rotate(r[2]) retainer();
  for (x = knob_xs)
    color("white") translate([x, knob_y, -1]) mirror([0, 0, 1]) knob();
}

if (part == "assembly") assembly();
else if (part == "front") front_shell();
else if (part == "back") back_plate();
else if (part == "port_plate") port_plate();
else if (part == "retainer") retainer();
else if (part == "knob") translate([0, 0, knob_h]) rotate([180, 0, 0]) knob();
