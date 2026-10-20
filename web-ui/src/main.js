import { Compartment, EditorState } from "@codemirror/state";
import { StreamLanguage } from "@codemirror/language";
import { haskell } from "@codemirror/legacy-modes/mode/haskell";
import { oneDark } from "@codemirror/theme-one-dark";
import { EditorView, keymap } from "@codemirror/view";
import { basicSetup } from "codemirror";
import "./style.css";

const initial = window.__ZEN_MASTER_INITIAL ?? {};
const language = document.querySelector("#language");
const statusBar = document.querySelector(".status-bar");
const fileStatus = document.querySelector("#file-status");
const runtimeStatus = document.querySelector("#runtime-status");
const fileDialog = document.querySelector("#file-dialog");
const directoryInput = document.querySelector("#directory");
const filenameInput = document.querySelector("#filename");
const fileList = document.querySelector("#file-list");
const dialogError = document.querySelector("#directory-error");
const confirmFile = document.querySelector("#confirm-file");
const filenameRow = document.querySelector("#filename-row");

let currentPath = initial.path ?? "";
let selectedPath = "";
let pickerMode = "open";
let suppressEdit = false;
let syncTimer;
const languageCompartment = new Compartment();

function send(kind, detail = {}) {
  if (window.ipc?.postMessage) {
    window.ipc.postMessage(JSON.stringify({ kind, ...detail }));
  }
}

function languageExtension(value) {
  return value === "tidal" ? StreamLanguage.define(haskell) : [];
}

function syncCode() {
  if (suppressEdit) return;
  clearTimeout(syncTimer);
  syncTimer = setTimeout(() => {
    send("edit", { code: view.state.doc.toString() });
    fileStatus.textContent = currentPath ? `Editing ${currentPath}` : "Unsaved changes";
  }, 160);
}

function runCode() {
  clearTimeout(syncTimer);
  const code = view.state.doc.toString();
  send("run", { code, language: language.value });
  fileStatus.textContent = currentPath ? `Running ${currentPath}` : "Running unsaved code";
}

const view = new EditorView({
  state: EditorState.create({
    doc: initial.code ?? "",
    extensions: [
      basicSetup,
      oneDark,
      languageCompartment.of(languageExtension(initial.language ?? "tidal")),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) syncCode();
      }),
      keymap.of([
        {
          key: "Mod-Enter",
          run: () => {
            runCode();
            return true;
          },
        },
        {
          key: "Mod-s",
          run: () => {
            saveCurrent();
            return true;
          },
        },
      ]),
    ],
  }),
  parent: document.querySelector("#editor"),
});

language.value = initial.language ?? "tidal";
currentPath = initial.path ?? "";

function showFileDialog(mode) {
  pickerMode = mode;
  selectedPath = "";
  dialogError.textContent = "";
  document.querySelector("#dialog-title").textContent =
    mode === "open" ? "Open code file" : "Save code file";
  filenameRow.hidden = mode === "open";
  confirmFile.textContent = mode === "open" ? "Open" : "Save";
  if (mode === "save" && currentPath) {
    const parts = currentPath.split(/[\\/]/);
    filenameInput.value = parts.pop() || "untitled.hs";
    directoryInput.value = parts.join("/") || initial.directory || "";
  } else {
    filenameInput.value = language.value === "mondo" ? "untitled.mondo" : "untitled.hs";
    directoryInput.value = initial.directory || "";
  }
  fileDialog.showModal();
  requestDirectory(directoryInput.value);
}

function requestDirectory(path) {
  send("list", { path });
}

function saveCurrent() {
  if (currentPath) {
    send("save", { path: currentPath, code: view.state.doc.toString(), language: language.value });
  } else {
    showFileDialog("save");
  }
}

function updateDirectory(data) {
  directoryInput.value = data.path ?? directoryInput.value;
  dialogError.textContent = data.error ?? "";
  fileList.replaceChildren();
  for (const entry of data.entries ?? []) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = `file-row${selectedPath === entry.path ? " selected" : ""}`;
    button.textContent = `${entry.directory ? "▸ " : "  "}${entry.name}`;
    button.addEventListener("click", () => {
      if (entry.directory) {
        selectedPath = "";
        requestDirectory(entry.path);
      } else {
        selectedPath = entry.path;
        if (pickerMode === "save") filenameInput.value = entry.name;
        updateDirectory(data);
      }
    });
    button.addEventListener("dblclick", () => {
      if (entry.directory) requestDirectory(entry.path);
      else if (pickerMode === "open") openSelected();
      else filenameInput.value = entry.name;
    });
    fileList.append(button);
  }
}

function openSelected() {
  if (!selectedPath) {
    dialogError.textContent = "Select a .hs or .mondo file first.";
    return;
  }
  send("open", { path: selectedPath });
}

function saveSelected() {
  const filename = filenameInput.value.trim();
  if (!filename) {
    dialogError.textContent = "Enter a file name.";
    return;
  }
  const folder = directoryInput.value.trim().replace(/[\\/]+$/, "");
  const path = folder ? `${folder}/${filename}` : filename;
  send("save", { path, code: view.state.doc.toString(), language: language.value });
}

document.querySelector("#run").addEventListener("click", runCode);
document.querySelector("#stop").addEventListener("click", () => send("stop"));
document.querySelector("#new-file").addEventListener("click", () => {
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: "" } });
  currentPath = "";
  fileStatus.textContent = "New unsaved file";
});
document.querySelector("#open-file").addEventListener("click", () => showFileDialog("open"));
document.querySelector("#save-file").addEventListener("click", saveCurrent);
document.querySelector("#save-as").addEventListener("click", () => showFileDialog("save"));
document.querySelector("#cancel-dialog").addEventListener("click", () => fileDialog.close());
document.querySelector("#up-directory").addEventListener("click", () => {
  const path = directoryInput.value.replace(/[\\/]+$/, "");
  const parent = path.replace(/[\\/][^\\/]+$/, "");
  requestDirectory(parent || path);
});
document.querySelector("#go-directory").addEventListener("click", () => requestDirectory(directoryInput.value));
confirmFile.addEventListener("click", () => {
  if (pickerMode === "open") openSelected();
  else saveSelected();
});
language.addEventListener("change", () => {
  view.dispatch({ effects: languageCompartment.reconfigure(languageExtension(language.value)) });
  send("language", { language: language.value });
});
document.querySelector("#sync").addEventListener("change", (event) => {
  send("sync", { enabled: event.target.checked });
});
document.querySelector("#bpm").addEventListener("change", (event) => {
  send("bpm", { bpm: Number(event.target.value) });
});

window.receiveRustMessage = (message) => {
  if (message.type === "directory") updateDirectory(message);
  if (message.type === "loaded") {
    suppressEdit = true;
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: message.code ?? "" } });
    suppressEdit = false;
    currentPath = message.path ?? "";
    language.value = message.language ?? "tidal";
    view.dispatch({ effects: languageCompartment.reconfigure(languageExtension(language.value)) });
    fileStatus.textContent = `Loaded ${currentPath}`;
    fileDialog.close();
  }
  if (message.type === "saved") {
    currentPath = message.path ?? currentPath;
    fileStatus.textContent = `Saved ${currentPath}`;
    fileDialog.close();
  }
  if (message.type === "file-error") {
    dialogError.textContent = message.error ?? "File operation failed.";
    if (!fileDialog.open) {
      runtimeStatus.textContent = message.error ?? "File operation failed.";
      statusBar.classList.add("error");
    }
  }
  if (message.type === "status") {
    runtimeStatus.textContent = message.error
      ? `Error: ${message.error}`
      : message.sampleWarning
        ? `Missing sample: ${message.sampleWarning}`
        : message.playing
          ? `Playing · ${message.synced ? "host clock" : `${message.bpm} BPM`}`
          : "Stopped";
    statusBar.classList.toggle("error", Boolean(message.error || message.sampleWarning));
    if (message.code !== undefined) {
      suppressEdit = true;
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: message.code } });
      suppressEdit = false;
    }
    document.querySelector("#sync").checked = Boolean(message.synced);
    document.querySelector("#bpm").value = String(message.bpm ?? 120);
  }
};

setInterval(() => send("status"), 350);
