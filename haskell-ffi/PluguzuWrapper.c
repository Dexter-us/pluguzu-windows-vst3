// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

#include <stdlib.h>
#include <stdio.h>
#include "HsFFI.h"

static int count = 0;

HsBool pluguzu_init(void){
  if (count++ > 0) return HS_BOOL_FALSE;
  int argc = 2;
  char *argv[] = { "+RTS", "-A32m", NULL };
  char **pargv = argv;

  // Initialize Haskell runtime
  hs_init(&argc, &pargv);
  printf("HS INIT!\n");

  // do any other initialization here and
  // return false if there was a problem
  return HS_BOOL_TRUE;
}

void pluguzu_exit(void){
  if (--count > 0) return;
  hs_exit();
  printf("HS EXIT!\n");
}
