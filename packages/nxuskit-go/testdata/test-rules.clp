; Test rules for CLIPS provider validation
; This file defines simple templates and rules for testing

; Template for a person
(deftemplate person
   (slot name (type STRING))
   (slot age (type INTEGER)))

; Template for classification result
(deftemplate classification
   (slot name (type STRING))
   (slot category (type SYMBOL)))

; Rule to classify seniors (age >= 65)
(defrule classify-senior
   (person (name ?n) (age ?a&:(>= ?a 65)))
   =>
   (assert (classification (name ?n) (category senior))))

; Rule to classify adults (age >= 18 and < 65)
(defrule classify-adult
   (person (name ?n) (age ?a&:(and (>= ?a 18) (< ?a 65))))
   =>
   (assert (classification (name ?n) (category adult))))

; Rule to classify minors (age < 18)
(defrule classify-minor
   (person (name ?n) (age ?a&:(< ?a 18)))
   =>
   (assert (classification (name ?n) (category minor))))
