/**
 * clips_wrapper.c - C wrapper functions for CLIPS macros
 *
 * This file provides C function wrappers for CLIPS macros that cannot be
 * called directly from Rust FFI. The macros are defined in evaluatn.h and
 * other CLIPS headers.
 *
 * CLIPS 6.4.2 is licensed under MIT No Attribution (MIT-0).
 * See: https://www.clipsrules.net/
 */

#include "clips.h"

/* Type checking and conversion wrappers */

/**
 * Get the type of a CLIPSValue
 * Wraps access to the type field in the header
 */
int clips_cv_type(CLIPSValue *cv) {
    if (cv == NULL || cv->header == NULL) {
        return VOID_TYPE;
    }
    return cv->header->type;
}

/**
 * Check if a CLIPSValue is of a specific type (using type bits)
 * Wraps the CVIsType macro from evaluatn.h
 */
bool clips_cv_is_type(CLIPSValue *cv, unsigned int typeBits) {
    if (cv == NULL || cv->header == NULL) {
        return false;
    }
    return ((1 << cv->header->type) & typeBits) ? true : false;
}

/**
 * Get integer value from CLIPSValue
 */
long long clips_cv_to_integer(CLIPSValue *cv) {
    if (cv == NULL || cv->integerValue == NULL) {
        return 0;
    }
    return cv->integerValue->contents;
}

/**
 * Get float value from CLIPSValue
 */
double clips_cv_to_float(CLIPSValue *cv) {
    if (cv == NULL || cv->floatValue == NULL) {
        return 0.0;
    }
    return cv->floatValue->contents;
}

/**
 * Get string/symbol value from CLIPSValue
 * Returns the lexeme contents (works for both STRING and SYMBOL types)
 */
const char *clips_cv_to_string(CLIPSValue *cv) {
    if (cv == NULL || cv->lexemeValue == NULL) {
        return NULL;
    }
    return cv->lexemeValue->contents;
}

/**
 * Get multifield value from CLIPSValue
 */
Multifield *clips_cv_to_multifield(const CLIPSValue *cv) {
    if (cv == NULL) {
        return NULL;
    }
    return cv->multifieldValue;
}

/**
 * Get fact pointer from CLIPSValue
 */
Fact *clips_cv_to_fact(const CLIPSValue *cv) {
    if (cv == NULL) {
        return NULL;
    }
    return cv->factValue;
}

/**
 * Get instance pointer from CLIPSValue
 */
Instance *clips_cv_to_instance(const CLIPSValue *cv) {
    if (cv == NULL) {
        return NULL;
    }
    return cv->instanceValue;
}

/**
 * Get external address from CLIPSValue
 */
void *clips_cv_to_external_address(CLIPSValue *cv) {
    if (cv == NULL || cv->externalAddressValue == NULL) {
        return NULL;
    }
    return cv->externalAddressValue->contents;
}

/* Multifield access wrappers */

/**
 * Get the length of a multifield
 */
size_t clips_multifield_length(const Multifield *mf) {
    if (mf == NULL) {
        return 0;
    }
    return mf->length;
}

/**
 * Get a slot from a multifield by index
 * Copies the value at the given index into the provided CLIPSValue
 */
void clips_multifield_slot(Multifield *mf, size_t index, CLIPSValue *result) {
    if (mf == NULL || result == NULL || index >= mf->length) {
        if (result != NULL) {
            result->voidValue = NULL;
        }
        return;
    }
    *result = mf->contents[index];
}

/* Defrule helper - CLIPS doesn't track per-rule firing count directly,
 * so we return 0 and let the Rust side handle tracking if needed */

/**
 * Get the number of times a rule has fired
 * Note: CLIPS doesn't maintain this count per-rule by default.
 * This is a placeholder that returns 0.
 */
unsigned long long clips_get_defrule_firings(Defrule *rule) {
    /* CLIPS doesn't track per-rule firing counts in the Defrule structure.
     * The watchFiring flag controls whether firings are logged, but
     * there's no counter. Return 0 for now. */
    (void)rule;
    return 0;
}

/**
 * Check if a defrule has the watch firing flag set
 */
bool clips_defrule_get_watch_firings(Defrule *rule) {
    if (rule == NULL) {
        return false;
    }
    return rule->watchFiring ? true : false;
}

/**
 * Set the watch firing flag on a defrule
 */
void clips_defrule_set_watch_firings(Defrule *rule, bool value) {
    if (rule != NULL) {
        rule->watchFiring = value ? 1 : 0;
    }
}

/* Value setters - create CLIPSValue structures */

/**
 * Set a CLIPSValue to void
 */
void clips_cv_set_void(CLIPSValue *cv) {
    if (cv != NULL) {
        cv->voidValue = NULL;
    }
}

/**
 * Set a CLIPSValue to an integer
 * Note: This requires creating an integer in the environment's symbol table
 */
void clips_cv_set_integer(Environment *env, CLIPSValue *cv, long long value) {
    if (cv != NULL && env != NULL) {
        cv->integerValue = CreateInteger(env, value);
    }
}

/**
 * Set a CLIPSValue to a float
 */
void clips_cv_set_float(Environment *env, CLIPSValue *cv, double value) {
    if (cv != NULL && env != NULL) {
        cv->floatValue = CreateFloat(env, value);
    }
}

/**
 * Set a CLIPSValue to a symbol
 */
void clips_cv_set_symbol(Environment *env, CLIPSValue *cv, const char *value) {
    if (cv != NULL && env != NULL && value != NULL) {
        cv->lexemeValue = CreateSymbol(env, value);
    }
}

/**
 * Set a CLIPSValue to a string
 */
void clips_cv_set_string(Environment *env, CLIPSValue *cv, const char *value) {
    if (cv != NULL && env != NULL && value != NULL) {
        cv->lexemeValue = CreateString(env, value);
    }
}

/**
 * Set a CLIPSValue to a fact
 */
void clips_cv_set_fact(CLIPSValue *cv, Fact *fact) {
    if (cv != NULL) {
        cv->factValue = fact;
    }
}

/**
 * Set a CLIPSValue to an instance
 */
void clips_cv_set_instance(CLIPSValue *cv, Instance *instance) {
    if (cv != NULL) {
        cv->instanceValue = instance;
    }
}

/**
 * Set a CLIPSValue to a multifield
 */
void clips_cv_set_multifield(CLIPSValue *cv, Multifield *mf) {
    if (cv != NULL) {
        cv->multifieldValue = mf;
    }
}

/**
 * Set a CLIPSValue to an external address
 */
void clips_cv_set_external_address(Environment *env, CLIPSValue *cv,
                                   void *address, unsigned short type) {
    if (cv != NULL && env != NULL) {
        cv->externalAddressValue = CreateExternalAddress(env, address, type);
    }
}

/* ========================================================================
 * Missing CLIPS API wrappers
 * These provide compatibility for functions that don't exist in CLIPS 6.4.2
 * ======================================================================== */

/**
 * Get the CLIPS version string
 * CLIPS 6.4.2 uses VERSION_STRING macro defined in constant.h
 */
const char *Version(void) { return VERSION_STRING; }

/**
 * Get the number of activations on the agenda
 * Iterates through the agenda to count activations
 * Note: The module parameter is accepted for API compatibility but currently
 * GetNextActivation iterates all activations regardless of module.
 */
long GetAgendaSize(Environment *env, const Defmodule *module) {
    long count = 0;
    Activation *act;

    if (env == NULL) {
        return 0;
    }

    /* Module parameter reserved for future filtering - currently unused */
    (void)module;

    /* Get first activation and count */
    act = GetNextActivation(env, NULL);
    while (act != NULL) {
        count++;
        act = GetNextActivation(env, act);
    }

    return count;
}

/**
 * Clear all activations from the agenda
 * Removes all activations for the specified module (or current module if NULL)
 */
void ClearAgenda(Environment *env, Defmodule *module) {
    Defmodule *target_module;

    if (env == NULL) {
        return;
    }

    /* If module is NULL, use the current module */
    target_module = (module != NULL) ? module : GetCurrentModule(env);

    /* DeleteAllActivations removes all activations from a specific module */
    DeleteAllActivations(target_module);
}

/**
 * Call a CLIPS function by name with arguments as a string
 * Uses FunctionCallBuilder for the actual call
 */
bool FunctionCall(Environment *env, const char *name, const char *args,
                  CLIPSValue *result) {
    FunctionCallBuilder *fcb;
    FunctionCallBuilderError err;

    if (env == NULL || name == NULL || result == NULL) {
        return false;
    }

    /* For simple calls without args, use FCBCall with no arguments */
    fcb = CreateFunctionCallBuilder(env, 0);
    if (fcb == NULL) {
        return false;
    }

    /* If args provided, we need to parse them - for now, only support no-arg
     * calls */
    if (args != NULL && args[0] != '\0') {
        /* Complex argument parsing would go here */
        /* For now, fallback to Eval which can handle the full expression */
        FCBDispose(fcb);

        /* Build the full expression: (name args) */
        char expr[1024];
        snprintf(expr, sizeof(expr), "(%s %s)", name, args);
        return (Eval(env, expr, result) == EE_NO_ERROR);
    }

    err = FCBCall(fcb, name, result);
    FCBDispose(fcb);

    return (err == FCBE_NO_ERROR);
}
