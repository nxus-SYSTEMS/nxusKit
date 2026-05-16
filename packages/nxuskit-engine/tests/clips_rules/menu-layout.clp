;;; Menu Layout Module
;;; Determines menu style based on screen configuration.

(defmodule MENU-LAYOUT)

(deftemplate MENU-LAYOUT::menu-input
    (slot items (type INTEGER))
    (slot depth (type INTEGER) (default 1)))

(deftemplate MENU-LAYOUT::menu-state
    (slot style (type SYMBOL))
    (slot max-depth (type INTEGER))
    (slot orientation (type SYMBOL)))

;;; Import screen-config from SCREEN-SIZE module so rules can reference it
(deftemplate MENU-LAYOUT::screen-config
    (slot category (type SYMBOL))
    (slot columns (type INTEGER))
    (slot breakpoint (type SYMBOL)))

(defrule MENU-LAYOUT::mobile-hamburger
    "Mobile screens get hamburger menu"
    (screen-config (category mobile))
    =>
    (assert (menu-state (style hamburger) (max-depth 2) (orientation vertical))))

(defrule MENU-LAYOUT::tablet-sidebar
    "Tablet screens get sidebar menu"
    (screen-config (category tablet))
    =>
    (assert (menu-state (style sidebar) (max-depth 3) (orientation vertical))))

(defrule MENU-LAYOUT::desktop-horizontal
    "Desktop screens get horizontal menu"
    (screen-config (category desktop))
    =>
    (assert (menu-state (style horizontal) (max-depth 4) (orientation horizontal))))
