# Makefile for nxuskit SDK C examples
#
# Usage:
#   make basic_chat    — build the basic chat example
#   make streaming     — build the streaming example
#   make all           — build all examples
#   make clean         — remove build artifacts
#
# Prerequisites:
#   The SDK must be extracted to ../../ (parent of examples/c/) or
#   set SDK_DIR to the SDK root directory.

# SDK root — default assumes this Makefile is at sdk-root/examples/c/
SDK_DIR ?= ../..
INCLUDE_DIR := $(SDK_DIR)/include
LIB_DIR := $(SDK_DIR)/lib

CC := cc
CFLAGS := -Wall -Wextra -I$(INCLUDE_DIR)

# Platform-specific link flags
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
    LDFLAGS := -L$(LIB_DIR) -lnxuskit -Wl,-rpath,$(LIB_DIR)
else ifeq ($(UNAME_S),Linux)
    LDFLAGS := -L$(LIB_DIR) -lnxuskit -Wl,-rpath,$(LIB_DIR) -lpthread -ldl -lm
else
    LDFLAGS := -L$(LIB_DIR) -lnxuskit
endif

EXAMPLES := basic_chat streaming

.PHONY: all clean $(EXAMPLES)

all: $(EXAMPLES)

basic_chat:
	@mkdir -p bin
	$(CC) $(CFLAGS) -o bin/basic_chat basic_chat.c $(LDFLAGS)

streaming:
	@mkdir -p bin
	$(CC) $(CFLAGS) -o bin/streaming streaming.c $(LDFLAGS)

clean:
	rm -rf bin
