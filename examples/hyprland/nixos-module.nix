# Path of Crafting 2 — Hyprland NixOS module snippet (Phase D.2)
#
# Drop into your NixOS Home-Manager (or system-level Hyprland) config:
#
#   imports = [ /path/to/poc2/examples/hyprland/nixos-module.nix ];
#
# The module appends 6 windowrulev2 lines to your Hyprland settings
# without touching anything else.

{ config, lib, ... }:

let
  poc2Class = "ai\\.anomaly\\.poc2";
in
{
  # For Home-Manager users:
  wayland.windowManager.hyprland.settings.windowrulev2 = lib.mkAfter [
    "float, class:^(${poc2Class})$"
    "pin, class:^(${poc2Class})$"
    "noborder, class:^(${poc2Class})$"
    "size 480 720, class:^(${poc2Class})$"
    "move 100% 0, class:^(${poc2Class})$"
    "opacity 0.95, class:^(${poc2Class})$"
  ];

  # Equivalent for system-level Hyprland modules using the
  # programs.hyprland.* nixos namespace would be:
  #
  # programs.hyprland.settings.windowrulev2 = ...
}
