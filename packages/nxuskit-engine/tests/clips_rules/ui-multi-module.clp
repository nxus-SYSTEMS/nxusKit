;;; UI Multi-Module Rule Base
;;; Combined rule base with SCREEN-SIZE, MENU-LAYOUT, and DATA-GRID modules.
;;; Used for testing focus-stack control (selective module execution).

;;; =====================================================================
;;; SCREEN-SIZE Module — classifies screen dimensions
;;; =====================================================================

(defmodule SCREEN-SIZE (export ?ALL))

(deftemplate SCREEN-SIZE::screen-input
    (slot width (type INTEGER))
    (slot height (type INTEGER)))

(deftemplate SCREEN-SIZE::screen-config
    (slot category (type SYMBOL))
    (slot columns (type INTEGER))
    (slot breakpoint (type SYMBOL)))

(defrule SCREEN-SIZE::classify-mobile
    "Classify screens narrower than 768px as mobile"
    (screen-input (width ?w&:(< ?w 768)))
    =>
    (assert (screen-config (category mobile) (columns 1) (breakpoint small))))

(defrule SCREEN-SIZE::classify-tablet
    "Classify screens 768-1023px as tablet"
    (screen-input (width ?w&:(>= ?w 768)&:(< ?w 1024)))
    =>
    (assert (screen-config (category tablet) (columns 2) (breakpoint medium))))

(defrule SCREEN-SIZE::classify-desktop
    "Classify screens 1024px and wider as desktop"
    (screen-input (width ?w&:(>= ?w 1024)))
    =>
    (assert (screen-config (category desktop) (columns 3) (breakpoint large))))

;;; =====================================================================
;;; MENU-LAYOUT Module — determines menu style from screen config
;;; =====================================================================

(defmodule MENU-LAYOUT (import SCREEN-SIZE ?ALL) (export ?ALL))

(deftemplate MENU-LAYOUT::menu-input
    (slot items (type INTEGER))
    (slot depth (type INTEGER) (default 1)))

(deftemplate MENU-LAYOUT::menu-state
    (slot style (type SYMBOL))
    (slot max-depth (type INTEGER))
    (slot orientation (type SYMBOL)))

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

;;; =====================================================================
;;; DATA-GRID Module — configures data grid from screen config
;;; =====================================================================

(defmodule DATA-GRID (import SCREEN-SIZE ?ALL) (export ?ALL))

(deftemplate DATA-GRID::grid-input
    (slot rows (type INTEGER))
    (slot enable-sorting (type SYMBOL) (default yes)))

(deftemplate DATA-GRID::grid-config
    (slot page-size (type INTEGER))
    (slot visible-columns (type INTEGER))
    (slot scroll-mode (type SYMBOL))
    (slot show-filters (type SYMBOL)))

(defrule DATA-GRID::mobile-grid
    "Mobile grid: small pages, few columns, virtual scroll"
    (screen-config (category mobile))
    =>
    (assert (grid-config (page-size 10) (visible-columns 2) (scroll-mode virtual) (show-filters no))))

(defrule DATA-GRID::tablet-grid
    "Tablet grid: medium pages, moderate columns"
    (screen-config (category tablet))
    =>
    (assert (grid-config (page-size 25) (visible-columns 4) (scroll-mode pagination) (show-filters yes))))

(defrule DATA-GRID::desktop-grid
    "Desktop grid: large pages, all columns"
    (screen-config (category desktop))
    =>
    (assert (grid-config (page-size 50) (visible-columns 8) (scroll-mode pagination) (show-filters yes))))
