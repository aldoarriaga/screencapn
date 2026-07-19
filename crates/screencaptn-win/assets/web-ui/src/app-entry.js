import {
  $createLineBreakNode,
  $createParagraphNode,
  $createTextNode,
  $getRoot,
  $isLineBreakNode,
  $isTextNode,
  COMMAND_PRIORITY_HIGH,
  createEditor,
  INSERT_LINE_BREAK_COMMAND,
  KEY_ENTER_COMMAND,
} from "lexical";
import { registerPlainText } from "@lexical/plain-text";
import { createEmptyHistoryState, registerHistory } from "@lexical/history";

window.SCREEN_CAPTN_LEXICAL = {
  $createLineBreakNode,
  $createParagraphNode,
  $createTextNode,
  $getRoot,
  $isLineBreakNode,
  $isTextNode,
  COMMAND_PRIORITY_HIGH,
  createEditor,
  createEmptyHistoryState,
  INSERT_LINE_BREAK_COMMAND,
  KEY_ENTER_COMMAND,
  registerHistory,
  registerPlainText,
};

import "../app.js";
