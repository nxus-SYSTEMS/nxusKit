;;; Data Grid Module
;;; Configures data grid layout based on screen configuration.

(defmodule DATA-GRID)

(deftemplate DATA-GRID::grid-input
    (slot rows (type INTEGER))
    (slot enable-sorting (type SYMBOL) (default yes)))

(deftemplate DATA-GRID::grid-config
    (slot page-size (type INTEGER))
    (slot visible-columns (type INTEGER))
    (slot scroll-mode (type SYMBOL))
    (slot show-filters (type SYMBOL)))

;;; Import screen-config from SCREEN-SIZE module
(deftemplate DATA-GRID::screen-config
    (slot category (type SYMBOL))
    (slot columns (type INTEGER))
    (slot breakpoint (type SYMBOL)))

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
