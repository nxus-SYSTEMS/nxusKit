;;; Screen Size Classification Module
;;; Classifies screen dimensions into device categories and column layouts.

(defmodule SCREEN-SIZE)

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
