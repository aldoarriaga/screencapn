(function () {
  "use strict";

  const host = (message) => {
    if (window.chrome && window.chrome.webview) {
      window.chrome.webview.postMessage(message);
    }
  };

  const diagnostic = (level, message, details) => {
    host({ type: "diagnostic", level, message: String(message || ""), details: details || null });
  };

  window.addEventListener("error", (event) => {
    diagnostic("error", event.message, {
      filename: event.filename,
      lineno: event.lineno,
      colno: event.colno,
      stack: event.error && event.error.stack ? String(event.error.stack) : null,
    });
  });

  window.addEventListener("unhandledrejection", (event) => {
    const reason = event.reason;
    diagnostic("error", reason && reason.message ? reason.message : reason, {
      stack: reason && reason.stack ? String(reason.stack) : null,
    });
  });

  const ICONS = window.SCREEN_CAPTN_ICONS || {};
  const container = document.getElementById("stage");
  const watermarkInput = document.getElementById("watermark-input");
  const stage = new Konva.Stage({
    container,
    width: window.innerWidth,
    height: window.innerHeight,
  });
  const committedLayer = new Konva.Layer({ listening: false });
  const selectionLayer = new Konva.Layer({ listening: false });
  const previewLayer = new Konva.Layer({ listening: false });
  const watermarkLayer = new Konva.Layer({ listening: false });
  const uiLayer = new Konva.Layer();
  stage.add(committedLayer);
  stage.add(selectionLayer);
  stage.add(previewLayer);
  stage.add(watermarkLayer);
  stage.add(uiLayer);

  const iconCache = new Map();
  const watermarkMeasureContext = document.createElement("canvas").getContext("2d");
  let state = null;
  let preview = null;
  let toolbarDrag = null;
  let regionResizeDrag = false;
  let toggleTween = null;
  let toolbarWasVisible = false;
  let toolbarAppearTweens = [];
  let themeDrawer = {
    open: false,
    clicking: false,
    node: null,
    icon: null,
    tween: null,
    iconTween: null,
    settleTween: null,
    pendingDone: null,
    revealAfterToggle: false,
    autoClosing: false,
  };
  let lastNumberingEnabled = false;
  let caretVisible = true;
  let caretForceVisibleUntil = 0;
  let hoveredSlider = null;
  let sliderHotzones = [];
  let colorSwatchHotzones = [];
  let renderScheduled = false;
  let previewRenderScheduled = false;
  let pendingState = null;
  let pointerMoveScheduled = false;
  let pendingPointerMove = null;
  const previewNodes = new Map();
  let previewDynamicGroup = null;
  const annotationNodeCache = new Map();
  const mosaicCanvasCache = new Map();
  let committedTarget = committedLayer;
  let pendingCommittedDiff = null;
  let textEditorSession = null;
  let textDraftTimer = null;
  const perf = {
    enabled: false,
    counters: new Map(),
  };
  const dirty = {
    ui: true,
    committed: true,
    selection: true,
    preview: true,
  };

  const tools = [
    { action: "grip", width: 36, icon: "grip" },
    { action: "numbering", width: 24, icon: "numbering" },
    { tool: "rectangle", width: 36, icon: "rectangle" },
    { tool: "oval", width: 36, icon: "oval" },
    { tool: "line", width: 36, icon: "line" },
    { tool: "arrow", width: 36, icon: "arrow" },
    { tool: "pen", width: 36, icon: "pen" },
    { tool: "highlighter", width: 36, icon: "highlighter" },
    { tool: "text", width: 36, icon: "text" },
    { tool: "tag", width: 36, icon: "tag" },
    { tool: "watermark", width: 36, icon: "watermark" },
    { tool: "mosaic", width: 36, icon: "mosaic" },
    { divider: true, width: 12 },
    { action: "undo", width: 36, icon: "undo" },
    { action: "copy", width: 36, icon: "copy" },
    { action: "save", width: 36, icon: "save" },
    { action: "cancel", width: 36, icon: "cancel" },
  ];

  const colors = ["#FF3B30", "#0A84FF", "#FFD60A", "#00C853", "#BF5AF2"];
  const TOOL_HOVER_EFFECT = {
    enabled: true,
    scale: 1.1,
    settleScale: 1.075,
    glowOpacity: 0.9,
    glowBlur: 12,
    inDuration: 0.08,
    settleDuration: 0.1,
    outDuration: 0.12,
  };
  const TOOLBAR_APPEAR = {
    offset: 56,
    overshoot: -0.4,
    riseDuration: 0.98,
    settleDuration: 0.12,
  };

  function scale() {
    return (state ? state.uiScale || 1 : 1) * viewportScale();
  }

  function viewportScale() {
    return viewportScaleFor(state);
  }

  function viewportScaleFor(value) {
    if (!value || !value.screen || !value.screen.width || !value.screen.height) {
      return 1;
    }
    const sx = window.innerWidth / value.screen.width;
    const sy = window.innerHeight / value.screen.height;
    return Math.min(sx, sy);
  }

  function physicalToCssRect(rect) {
    const s = viewportScale();
    return {
      x: rect.x * s,
      y: rect.y * s,
      width: rect.width * s,
      height: rect.height * s,
    };
  }

  function cssToPhysicalPoint(point) {
    const s = viewportScale();
    return {
      x: point.x / s,
      y: point.y / s,
      rawX: point.x,
      rawY: point.y,
    };
  }

  function palette() {
    return paletteFor(state && state.theme === "dark" ? "dark" : "light");
  }

  function paletteFor(theme) {
    const dark = theme === "dark";
    return dark
      ? {
          bg: "#1A1A1A",
          bgEdgeTop: "#2A2A2A",
          bgEdgeBottom: "#050505",
          icon: "#B3B3B3",
          selected: "#333333",
          borderTop: "rgba(255,255,255,0.16)",
          borderBottom: "rgba(0,0,0,0.80)",
          separator: "#4B4B4B",
          submenuBg: "#1A1A1A",
          slider: "#B3B3B3",
          swatchBorder: "#F2F2F2",
        }
      : {
          bg: "#F2F2F2",
          bgEdgeTop: "#FFFFFF",
          bgEdgeBottom: "#D6D6D6",
          icon: "#4D4D4D",
          selected: "#FFFFFF",
          borderTop: "#FFFFFF",
          borderBottom: "#D2D2D2",
          separator: "#C8C8C8",
          submenuBg: "#F2F2F2",
          slider: "#4D4D4D",
          swatchBorder: "#4D4D4D",
        };
  }

  function hexColor(color) {
    if (!color) {
      return "#FF3B30";
    }
    const part = (n) => Math.max(0, Math.min(255, n || 0)).toString(16).padStart(2, "0");
    return `#${part(color.r)}${part(color.g)}${part(color.b)}`.toUpperCase();
  }

  function hexToWebColor(hex) {
    const value = String(hex || "#FF3B30").replace("#", "");
    const part = (start, fallback) => {
      const parsed = parseInt(value.slice(start, start + 2), 16);
      return Number.isFinite(parsed) ? parsed : fallback;
    };
    return { r: part(0, 255), g: part(2, 59), b: part(4, 48), a: 255 };
  }

  function activeColor() {
    if (state && state.activeSubmenu === "watermark") {
      return hexColor(state.watermarkColor);
    }
    const selected = selectedAnnotation();
    if (selected && selected.stroke && selected.stroke.color) {
      return hexColor(selected.stroke.color);
    }
    return hexColor(state && state.currentStroke && state.currentStroke.color);
  }

  function currentWidth() {
    const selected = selectedAnnotation();
    if (selected && selected.stroke && selected.stroke.width) {
      return selected.stroke.width;
    }
    return state && state.currentStroke ? state.currentStroke.width || 1 : 1;
  }

  function currentFontSize() {
    const selected = selectedAnnotation();
    const kind = selected && selected.kind;
    if (state && state.activeSubmenu === "watermark") {
      return state.fontSize || 27;
    }
    if ((kind && kind.type === "text") || (kind && kind.type === "tag")) {
      return kind.fontSize || state.fontSize || 27;
    }
    return state ? state.fontSize || 27 : 27;
  }

  function sliderAllowedForActiveTool(slider) {
    if (!slider || !state || !state.activeSubmenu || !state.captureRegion) {
      return false;
    }
    const tool = state.activeSubmenu;
    if (slider.kind === "font") {
      return tool === "text" || tool === "tag" || tool === "watermark";
    }
    return ["rectangle", "oval", "line", "arrow", "pen", "highlighter", "tag"].includes(tool);
  }

  function defaultSliderForActiveTool() {
    if (sliderAllowedForActiveTool(hoveredSlider)) {
      return hoveredSlider;
    }
    if (!state || !state.activeSubmenu || !state.captureRegion) {
      return null;
    }
    const tool = state.activeSubmenu;
    if (tool === "text" || tool === "tag" || tool === "watermark") {
      return { kind: "font", min: 27, max: 56 };
    }
    if (["rectangle", "oval", "line", "arrow", "pen", "highlighter"].includes(tool)) {
      return { kind: "stroke", min: 1, max: 24 };
    }
    return null;
  }

  function keyboardSliderForActiveTool() {
    if (hoveredSlider && hoveredSlider.kind === "stroke" && sliderAllowedForActiveTool(hoveredSlider)) {
      return hoveredSlider;
    }
    if (!state || !state.activeSubmenu || !state.captureRegion) {
      return null;
    }
    const tool = state.activeSubmenu;
    if (["rectangle", "oval", "line", "arrow", "pen", "highlighter"].includes(tool)) {
      return { kind: "stroke", min: 1, max: 24 };
    }
    if (tool === "tag" && hoveredSlider && hoveredSlider.kind === "stroke") {
      return { kind: "stroke", min: 6, max: 24 };
    }
    return null;
  }

  function currentSliderValue(kind) {
    return kind === "font" ? currentFontSize() : currentWidth();
  }

  function adjustSlider(slider, direction, fast) {
    if (!sliderAllowedForActiveTool(slider)) {
      return false;
    }
    const step = (fast ? 3 : 1) * (slider.kind === "font" ? 1 : 1);
    const current = currentSliderValue(slider.kind);
    const value = Math.max(slider.min, Math.min(slider.max, Math.round(current + direction * step)));
    command(slider.kind === "font" ? "setFontSize" : "setStrokeWidth", { value });
    return true;
  }

  function adjustSliderFromWheel(slider, event) {
    if (!event || event.ctrlKey) {
      return false;
    }
    const direction = event.deltaY < 0 ? 1 : -1;
    if (!adjustSlider(slider, direction, event.shiftKey)) {
      return false;
    }
    event.preventDefault();
    if (event.stopPropagation) {
      event.stopPropagation();
    }
    return true;
  }

  function eventStagePoint(event) {
    const rect = container.getBoundingClientRect();
    return { x: event.clientX - rect.left, y: event.clientY - rect.top };
  }

  function stagePointFromKonvaEvent(evt) {
    const nativeEvent = evt && evt.evt;
    const touch =
      nativeEvent && nativeEvent.touches && nativeEvent.touches.length
        ? nativeEvent.touches[0]
        : nativeEvent && nativeEvent.changedTouches && nativeEvent.changedTouches.length
          ? nativeEvent.changedTouches[0]
          : null;
    return touch ? eventStagePoint(touch) : nativeEvent ? eventStagePoint(nativeEvent) : pointerPosition();
  }

  function sliderAtStagePoint(point) {
    if (!point) {
      return null;
    }
    for (let i = sliderHotzones.length - 1; i >= 0; i--) {
      const zone = sliderHotzones[i];
      if (pointInRect(point, zone.rect, 0)) {
        return zone.slider;
      }
    }
    return null;
  }

  function colorAtSwatchPoint(point) {
    if (!point) {
      return null;
    }
    for (let i = colorSwatchHotzones.length - 1; i >= 0; i -= 1) {
      const zone = colorSwatchHotzones[i];
      if (pointInRect(point, zone.rect, 0)) {
        return zone.color;
      }
    }
    return null;
  }

  function selectedAnnotation() {
    if (!state || !state.selectedAnnotationId || !Array.isArray(state.annotations)) {
      return null;
    }
    return state.annotations.find((annotation) => annotation.id === state.selectedAnnotationId) || null;
  }

  function renderStyle() {
    return (state && state.renderStyle) || {};
  }

  function physicalScalar(value) {
    return value * viewportScale();
  }

  function physicalToCssPoint(point) {
    const s = viewportScale();
    return {
      x: point.x * s,
      y: point.y * s,
    };
  }

  function physicalStrokeWidth(width) {
    return Math.max(1, physicalScalar(width || 1));
  }

  function strokeColor(annotation) {
    return hexColor(annotation && annotation.stroke && annotation.stroke.color);
  }

  function strokeWidth(annotation) {
    return physicalStrokeWidth(annotation && annotation.stroke && annotation.stroke.width);
  }

  function strokeOpacity(annotation) {
    return annotation && annotation.stroke && annotation.stroke.opacity != null ? annotation.stroke.opacity : 1;
  }

  function flattenPhysicalPoints(points) {
    const out = [];
    for (const point of points || []) {
      const p = physicalToCssPoint(point);
      out.push(p.x, p.y);
    }
    return out;
  }

  function contrastTextColor(hex) {
    const value = hex.replace("#", "");
    const r = parseInt(value.slice(0, 2), 16);
    const g = parseInt(value.slice(2, 4), 16);
    const b = parseInt(value.slice(4, 6), 16);
    return r * 0.299 + g * 0.587 + b * 0.114 > 160 ? "#000000" : "#FFFFFF";
  }

  function toolbarRect() {
    if (!state || !state.captureRegion || !state.toolbar) {
      return null;
    }
    return physicalToCssRect(state.toolbar);
  }

  function recolorSvg(svg, color) {
    return svg
      .replace(/#4d4d4d/gi, color)
      .replace(/#b3b3b3/gi, color)
      .replace(/#000000/gi, color)
      .replace(/#000/gi, color);
  }

  function iconSource(name, color) {
    const key = name === "grip" ? (state.theme === "dark" ? "gripDark" : "gripLight") : name;
    const svg = ICONS[key];
    if (!svg) {
      return null;
    }
    if (name === "grip") {
      return "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg);
    }
    return "data:image/svg+xml;charset=utf-8," + encodeURIComponent(recolorSvg(svg, color));
  }

  function drawIcon(group, name, x, y, size, color, opacity) {
    const src = iconSource(name, color);
    if (!src) {
      return null;
    }
    const cacheKey = `${name}:${color}:${state.theme}`;
    let image = iconCache.get(cacheKey);
    const node = new Konva.Image({ x, y, width: size, height: size, opacity: opacity == null ? 1 : opacity, name: "ui" });
    group.add(node);
    if (image && image.complete) {
      node.image(image);
      return node;
    }
    image = new Image();
    iconCache.set(cacheKey, image);
    image.onload = () => {
      node.image(image);
      uiLayer.batchDraw();
    };
    image.src = src;
    return node;
  }

  function drawHoverableToolbarIcon(button, icon, x, y, size, normalColor, hoverColor, selected, s) {
    const iconGroup = new Konva.Group({
      x: x + size / 2,
      y: y + size / 2,
      offsetX: size / 2,
      offsetY: size / 2,
      name: "ui",
    });
    button.add(iconGroup);
    drawIcon(iconGroup, icon, 0, 0, size, normalColor, 1);

    let glowIcon = null;
    if (TOOL_HOVER_EFFECT.enabled && !selected) {
      glowIcon = drawIcon(iconGroup, icon, 0, 0, size, hoverColor, 0);
      if (glowIcon) {
        glowIcon.shadowColor(hoverColor);
        glowIcon.shadowBlur(TOOL_HOVER_EFFECT.glowBlur * s);
        glowIcon.shadowOpacity(0.72);
        glowIcon.shadowForStrokeEnabled(false);
      }
    }

    let tween = null;
    let settleTween = null;
    const stopTweens = () => {
      if (tween) {
        tween.destroy();
        tween = null;
      }
      if (settleTween) {
        settleTween.destroy();
        settleTween = null;
      }
    };

    return {
      enter() {
        if (!TOOL_HOVER_EFFECT.enabled || selected) {
          return;
        }
        stopTweens();
        if (glowIcon) {
          glowIcon.opacity(TOOL_HOVER_EFFECT.glowOpacity);
        }
        tween = new Konva.Tween({
          node: iconGroup,
          duration: TOOL_HOVER_EFFECT.inDuration,
          easing: Konva.Easings.EaseOut,
          scaleX: TOOL_HOVER_EFFECT.scale,
          scaleY: TOOL_HOVER_EFFECT.scale,
          onFinish: () => {
            settleTween = new Konva.Tween({
              node: iconGroup,
              duration: TOOL_HOVER_EFFECT.settleDuration,
              easing: Konva.Easings.EaseInOut,
              scaleX: TOOL_HOVER_EFFECT.settleScale,
              scaleY: TOOL_HOVER_EFFECT.settleScale,
            });
            settleTween.play();
          },
        });
        tween.play();
      },
      leave() {
        if (!TOOL_HOVER_EFFECT.enabled || selected) {
          return;
        }
        stopTweens();
        if (glowIcon) {
          glowIcon.opacity(0);
        }
        tween = new Konva.Tween({
          node: iconGroup,
          duration: TOOL_HOVER_EFFECT.outDuration,
          easing: Konva.Easings.EaseInOut,
          scaleX: 1,
          scaleY: 1,
        });
        tween.play();
      },
    };
  }

  function isUiTarget(target) {
    let node = target;
    while (node) {
      if (node.hasName && node.hasName("ui")) {
        return true;
      }
      node = node.getParent && node.getParent();
    }
    return false;
  }

  function command(type, extra) {
    host(Object.assign({ type }, extra || {}));
  }

  function showCaretNow() {
    caretVisible = true;
    caretForceVisibleUntil = Date.now() + 650;
  }

  function measure(name, fn) {
    if (!perf.enabled || !window.performance) {
      return fn();
    }
    const start = performance.now();
    try {
      return fn();
    } finally {
      const duration = performance.now() - start;
      const current = perf.counters.get(name) || { count: 0, total: 0, max: 0 };
      current.count += 1;
      current.total += duration;
      current.max = Math.max(current.max, duration);
      perf.counters.set(name, current);
      if (current.count % 120 === 0) {
        console.debug(
          `[ScreenCaptn perf] ${name}: avg=${(current.total / current.count).toFixed(2)}ms max=${current.max.toFixed(2)}ms`
        );
      }
    }
  }

  function markDirty(parts) {
    if (!parts) {
      dirty.ui = true;
      dirty.committed = true;
      dirty.selection = true;
      dirty.preview = true;
      return;
    }
    dirty.ui = dirty.ui || !!parts.ui;
    dirty.committed = dirty.committed || !!parts.committed;
    dirty.selection = dirty.selection || !!parts.selection;
    dirty.preview = dirty.preview || !!parts.preview;
  }

  function stopToolbarAppearTweens() {
    for (const tween of toolbarAppearTweens) {
      if (tween) {
        tween.destroy();
      }
    }
    toolbarAppearTweens = [];
  }

  function shouldAnimateToolbarAppearance(rect) {
    if (!rect) {
      toolbarWasVisible = false;
      stopToolbarAppearTweens();
      return false;
    }
    if (toolbarWasVisible) {
      return false;
    }
    toolbarWasVisible = true;
    return true;
  }

  function toolbarAppearEase(t) {
    const clamped = Math.max(0, Math.min(1, t));
    return clamped * clamped * (3 - 2 * clamped);
  }

  function animateToolbarSurface(group) {
    const s = scale();
    const baseY = group.y();
    group.opacity(0);
    group.y(baseY + TOOLBAR_APPEAR.offset * s);
    const rise = new Konva.Tween({
      node: group,
      duration: TOOLBAR_APPEAR.riseDuration,
      easing: toolbarAppearEase,
      opacity: 1,
      y: baseY + TOOLBAR_APPEAR.overshoot * s,
      onFinish: () => {
        const settle = new Konva.Tween({
          node: group,
          duration: TOOLBAR_APPEAR.settleDuration,
          easing: toolbarAppearEase,
          y: baseY,
        });
        toolbarAppearTweens.push(settle);
        settle.play();
      },
    });
    toolbarAppearTweens.push(rise);
    rise.play();
  }

  function scheduleRender(nextState, parts) {
    if (nextState) {
      pendingState = nextState;
    }
    if (parts) {
      markDirty(parts);
    } else if (!nextState) {
      markDirty();
    }
    if (renderScheduled) {
      return;
    }
    renderScheduled = true;
    requestAnimationFrame(() => {
      renderScheduled = false;
      if (pendingState) {
        const next = pendingState;
        pendingState = null;
        markDirty(dirtyForStateChange(state, next));
        state = next;
      }
      measure("dirty-render", renderDirty);
    });
  }

  function dirtyForStateChange(previous, next) {
    if (!previous || !next) {
      return { ui: true, committed: true, selection: true, preview: true };
    }
    const viewportChanged = viewportScaleFor(previous) !== viewportScaleFor(next) || previous.uiScale !== next.uiScale;
    return {
      ui: viewportChanged || uiRenderSignature(previous) !== uiRenderSignature(next),
      committed: viewportChanged || committedRenderSignature(previous) !== committedRenderSignature(next),
      selection: viewportChanged || selectionRenderSignature(previous) !== selectionRenderSignature(next),
      preview: false,
    };
  }

  function uiRenderSignature(value) {
    const selected = selectedAnnotationFromState(value);
    return JSON.stringify({
      activeTool: value.activeTool,
      activeSubmenu: value.activeSubmenu,
      theme: value.theme,
      uiScale: value.uiScale,
      captureRegion: value.captureRegion,
      toolbar: value.toolbar,
      regionControls: value.regionControls,
      currentStroke: value.currentStroke,
      fontSize: value.fontSize,
      penMode: value.penMode,
      highlighterShape: value.highlighterShape,
      textFilled: value.textFilled,
      numberingEnabled: value.numberingEnabled,
      watermarkMode: value.watermarkMode,
      watermarkText: value.watermarkText,
      watermarkColor: value.watermarkColor,
      watermarkImageUrl: value.watermarkImageUrl,
      watermarkImageDataUrl: value.watermarkImageDataUrl,
      watermarkDateEnabled: value.watermarkDateEnabled,
      selectedAnnotationId: value.selectedAnnotationId,
      selectedStroke: selected && selected.stroke,
      selectedKind: selected && selected.kind,
      renderStyle: value.renderStyle,
      viewportScale: viewportScaleFor(value),
    });
  }

  function committedRenderSignature(value) {
    return JSON.stringify({
      annotations: value.annotations,
      watermarkText: value.watermarkText,
      watermarkColor: value.watermarkColor,
      watermarkImageUrl: value.watermarkImageUrl,
      watermarkImageDataUrl: value.watermarkImageDataUrl,
      watermarkDateEnabled: value.watermarkDateEnabled,
      watermarkMode: value.watermarkMode,
      renderStyle: value.renderStyle,
      editingTextId: value.editingTextId,
      editingTextCaret: value.editingTextCaret,
      editingStepNumberId: value.editingStepNumberId,
      viewportScale: viewportScaleFor(value),
      uiScale: value.uiScale,
    });
  }

  function selectionRenderSignature(value) {
    const selected = selectedAnnotationFromState(value);
    return JSON.stringify({
      selectedAnnotationId: value.selectedAnnotationId,
      selected,
      renderStyle: value.renderStyle,
      viewportScale: viewportScaleFor(value),
      uiScale: value.uiScale,
    });
  }

  function selectedAnnotationFromState(value) {
    if (!value || !Array.isArray(value.annotations) || value.selectedAnnotationId == null) {
      return null;
    }
    return value.annotations.find((annotation) => annotation.id === value.selectedAnnotationId) || null;
  }

  function schedulePreviewRender() {
    if (previewRenderScheduled) {
      return;
    }
    previewRenderScheduled = true;
    requestAnimationFrame(() => {
      previewRenderScheduled = false;
      measure("preview-render", renderPreview);
      previewLayer.batchDraw();
    });
  }

  function schedulePointerMove(point) {
    pendingPointerMove = point;
    if (pointerMoveScheduled) {
      return;
    }
    pointerMoveScheduled = true;
    requestAnimationFrame(() => {
      pointerMoveScheduled = false;
      if (!pendingPointerMove) {
        return;
      }
      const point = pendingPointerMove;
      pendingPointerMove = null;
      command("pointerMove", cssToPhysicalPoint(point));
    });
  }

  function flushPointerMove() {
    if (!pendingPointerMove) {
      return;
    }
    const point = pendingPointerMove;
    pendingPointerMove = null;
    command("pointerMove", cssToPhysicalPoint(point));
  }

  function lexicalApi() {
    return window.SCREEN_CAPTN_LEXICAL || null;
  }

  function activeTextAnnotation() {
    if (!state || state.editingTextId == null || !Array.isArray(state.annotations)) {
      return null;
    }
    return state.annotations.find((annotation) => annotation.id === state.editingTextId) || null;
  }

  function ensureLexicalEditorRoot() {
    let root = document.getElementById("lexical-text-editor");
    if (!root) {
      root = document.createElement("div");
      root.id = "lexical-text-editor";
      root.className = "lexical-text-editor";
      root.spellcheck = false;
      root.setAttribute("contenteditable", "true");
      root.setAttribute("role", "textbox");
      root.setAttribute("aria-multiline", "true");
      document.body.appendChild(root);
    }
    return root;
  }

  function annotationTextValue(annotation) {
    const kind = annotation && annotation.kind ? annotation.kind : {};
    if (kind.type === "text") {
      return kind.text || "";
    }
    if (kind.type === "tag") {
      return kind.label || "";
    }
    return "";
  }

  function replaceAnnotationText(annotation, text) {
    const kind = annotation && annotation.kind ? annotation.kind : {};
    if (kind.type === "text") {
      return autoGrowFramedTextAnnotation({ ...annotation, kind: { ...kind, text } }, text);
    }
    if (kind.type === "tag") {
      return { ...annotation, kind: { ...kind, label: text } };
    }
    return annotation;
  }

  function autoGrowFramedTextAnnotation(annotation, text) {
    const kind = annotation && annotation.kind ? annotation.kind : {};
    if (kind.type !== "text" || !kind.framed || !annotation.bounds) {
      return annotation;
    }
    const scaleValue = Math.max(0.0001, viewportScale());
    const bounds = physicalToCssRect(annotation.bounds);
    const fontSize = physicalScalar(kind.fontSize || state.fontSize || 27);
    const padding = physicalScalar(8);
    const maxWidth = Math.max(1, bounds.width - padding * 2);
    const lineHeight = fontSize * 1.22;
    const visualLines = textVisualLines(text, fontSize, maxWidth);
    const requiredCssHeight = padding * 2 + Math.max(fontSize, visualLines.length * lineHeight);
    const requiredPhysicalHeight = Math.ceil(requiredCssHeight / scaleValue);
    if (requiredPhysicalHeight <= annotation.bounds.height + 0.5) {
      return annotation;
    }
    return {
      ...annotation,
      bounds: {
        ...annotation.bounds,
        height: requiredPhysicalHeight,
      },
    };
  }

  function updateLocalAnnotation(id, update) {
    if (!state || !Array.isArray(state.annotations)) {
      return;
    }
    let changed = null;
    const annotations = state.annotations.map((annotation) => {
      if (annotation.id !== id) {
        return annotation;
      }
      changed = update(annotation);
      return changed || annotation;
    });
    if (!changed) {
      return;
    }
    state = { ...state, annotations };
    pendingState = state;
    queueCommittedDiff(new Set(), [], [changed]);
    scheduleRender(state, { committed: true, selection: true, ui: true });
  }

  function updateLocalAnnotationText(id, text) {
    if (!state || !Array.isArray(state.annotations)) {
      return;
    }
    updateLocalAnnotation(id, (annotation) => replaceAnnotationText(annotation, text));
  }

  function selectedTextAnnotation() {
    const annotation = selectedAnnotation();
    return annotation && annotation.kind && annotation.kind.type === "text" ? annotation : null;
  }

  function applyLocalTextFilled(filled) {
    const annotation = selectedTextAnnotation();
    if (!annotation) {
      return;
    }
    updateLocalAnnotation(annotation.id, (current) => ({
      ...current,
      kind: { ...current.kind, filled },
    }));
  }

  function applyLocalColor(color) {
    if (!colors.includes(color)) {
      return;
    }
    const annotation = selectedAnnotation();
    if (!annotation) {
      return;
    }
    updateLocalAnnotation(annotation.id, (current) => ({
      ...current,
      stroke: { ...current.stroke, color: hexToWebColor(color) },
    }));
  }

  function sendTextDraft(id, text) {
    window.clearTimeout(textDraftTimer);
    textDraftTimer = window.setTimeout(() => {
      command("setTextDraft", { id, text });
    }, 80);
  }

  function flushTextDraft() {
    if (!textEditorSession || textEditorSession.lastText == null) {
      return;
    }
    window.clearTimeout(textDraftTimer);
    command("setTextDraft", { id: textEditorSession.id, text: textEditorSession.lastText });
  }

  function uiInteractionRect() {
    const toolbar = toolbarRect();
    if (!toolbar || !state || !state.activeSubmenu) {
      return toolbar;
    }
    const submenu = state.captureRegion ? submenuLayout(toolbar, state.activeSubmenu, scale()) : null;
    if (!submenu) {
      return toolbar;
    }
    const left = Math.min(toolbar.x, submenu.x);
    const top = Math.min(toolbar.y, submenu.y);
    const right = Math.max(toolbar.x + toolbar.width, submenu.x + submenu.width);
    const bottom = Math.max(toolbar.y + toolbar.height, submenu.y + submenu.height);
    return { x: left, y: top, width: right - left, height: bottom - top };
  }

  function pointInUiControls(point) {
    const rect = uiInteractionRect();
    return !!rect && pointInRect(point, rect, 4 * scale());
  }

  function syncEditorPointerEvents(point) {
    if (!textEditorSession || !textEditorSession.root) {
      return;
    }
    textEditorSession.root.style.pointerEvents = point && pointInUiControls(point) ? "none" : "auto";
  }

  function setLexicalText(editor, text, caretIndex) {
    const api = lexicalApi();
    if (!api) {
      return;
    }
    const {
      $createLineBreakNode,
      $createParagraphNode,
      $createTextNode,
      $getRoot,
    } = api;
    editor.update(() => {
      const root = $getRoot();
      root.clear();
      const paragraph = $createParagraphNode();
      const parts = String(text || "").split("\n");
      parts.forEach((part, index) => {
        if (part) {
          paragraph.append($createTextNode(part));
        }
        if (index < parts.length - 1) {
          paragraph.append($createLineBreakNode());
        }
      });
      root.append(paragraph);
      selectLexicalTextOffset(paragraph, caretIndex == null ? String(text || "").length : caretIndex);
    });
  }

  function selectLexicalTextOffset(paragraph, caretIndex) {
    const api = lexicalApi();
    if (!api || !paragraph || !paragraph.getChildren) {
      paragraph && paragraph.selectEnd && paragraph.selectEnd();
      return;
    }
    const { $isLineBreakNode, $isTextNode } = api;
    let remaining = Math.max(0, caretIndex || 0);
    let lastText = null;
    for (const child of paragraph.getChildren()) {
      if ($isTextNode(child)) {
        const length = child.getTextContentSize();
        if (remaining <= length) {
          child.select(remaining, remaining);
          return;
        }
        remaining -= length;
        lastText = child;
      } else if ($isLineBreakNode(child)) {
        if (remaining <= 0) {
          if (lastText) {
            const length = lastText.getTextContentSize();
            lastText.select(length, length);
          } else {
            paragraph.selectStart();
          }
          return;
        }
        remaining -= 1;
      }
    }
    if (lastText) {
      const length = lastText.getTextContentSize();
      lastText.select(length, length);
    } else {
      paragraph.selectEnd();
    }
  }

  function styleLexicalOverlay(root, annotation) {
    const metrics = editingTextMetrics(annotation);
    if (!metrics) {
      root.style.display = "none";
      return;
    }
    const kind = annotation.kind || {};
    const framed = kind.type === "text" && !!kind.framed;
    const color = strokeColor(annotation);
    const filled = kind.type === "text" && !!kind.filled;
    const textColor = kind.type === "tag" ? "#000000" : filled ? contrastTextColor(color) : color;
    root.style.display = "block";
    root.style.left = `${metrics.originX}px`;
    root.style.top = `${metrics.originY}px`;
    if (Number.isFinite(metrics.maxWidth)) {
      root.style.width = `${Math.max(1, metrics.maxWidth)}px`;
      root.style.minWidth = "2px";
      root.style.maxWidth = `${Math.max(1, metrics.maxWidth)}px`;
    } else {
      const lines = String(annotationTextValue(annotation) || "").split(/\r?\n/);
      const width = Math.max(
        metrics.fontSize * 0.45,
        lines.reduce((max, line) => Math.max(max, measureTextWidth(line, metrics.fontSize, false)), 0)
      );
      root.style.width = `${Math.ceil(width + 2)}px`;
      root.style.minWidth = "2px";
      root.style.maxWidth = "none";
    }
    root.style.minHeight = `${metrics.lineHeight}px`;
    root.style.fontFamily = "Segoe UI, Arial, sans-serif";
    root.style.fontSize = `${metrics.fontSize}px`;
    root.style.lineHeight = String(metrics.lineHeight / metrics.fontSize);
    root.style.color = textColor;
    root.style.padding = "0";
    root.style.whiteSpace = framed || kind.type === "tag" ? "pre-wrap" : "pre";
    root.style.overflowWrap = framed || kind.type === "tag" ? "break-word" : "normal";
    root.style.transform = "none";
    root.style.transformOrigin = "left top";
  }

  function syncTextEditorOverlay() {
    const annotation = activeTextAnnotation();
    if (!annotation || !lexicalApi()) {
      destroyTextEditor(false);
      return;
    }
    if (!textEditorSession || textEditorSession.id !== annotation.id) {
      openTextEditor(annotation, state.editingTextCaret || 0);
      return;
    }
    styleLexicalOverlay(textEditorSession.root, annotation);
  }

  function openTextEditor(annotation, caretIndex) {
    destroyTextEditor(false);
    const api = lexicalApi();
    if (!api) {
      return;
    }
    const root = ensureLexicalEditorRoot();
    const editor = api.createEditor({
      namespace: "ScreenCaptnTextEditor",
      onError(error) {
        diagnostic("error", error && error.message ? error.message : error, { source: "lexical" });
      },
    });
    const originalText = annotationTextValue(annotation);
    textEditorSession = {
      id: annotation.id,
      root,
      editor,
      originalText,
      lastText: originalText,
      unregister: [],
      committing: false,
    };
    styleLexicalOverlay(root, annotation);
    editor.setRootElement(root);
    textEditorSession.unregister.push(api.registerPlainText(editor));
    textEditorSession.unregister.push(api.registerHistory(editor, api.createEmptyHistoryState(), 250));
    textEditorSession.unregister.push(
      editor.registerCommand(
        api.KEY_ENTER_COMMAND,
        (event) => {
          if (event && event.shiftKey) {
            event.preventDefault();
            return editor.dispatchCommand(api.INSERT_LINE_BREAK_COMMAND, false);
          }
          if (event) {
            event.preventDefault();
          }
          commitTextEditor(true);
          return true;
        },
        api.COMMAND_PRIORITY_HIGH
      )
    );
    textEditorSession.unregister.push(
      editor.registerUpdateListener(({ editorState }) => {
        if (!textEditorSession || textEditorSession.committing) {
          return;
        }
        editorState.read(() => {
          const text = api.$getRoot().getTextContent();
          textEditorSession.lastText = text;
          updateLocalAnnotationText(annotation.id, text);
          sendTextDraft(annotation.id, text);
        });
      })
    );
    root.onkeydown = (event) => {
      if (event.key === "Escape") {
        event.preventDefault();
        cancelTextEditor();
      }
    };
    root.onblur = () => {
      window.setTimeout(() => {
        if (textEditorSession && document.activeElement !== textEditorSession.root) {
          commitTextEditor(false);
        }
      }, 0);
    };
    setLexicalText(editor, originalText, caretIndex);
    requestAnimationFrame(() => {
      if (!textEditorSession || textEditorSession.id !== annotation.id) {
        return;
      }
      root.focus({ preventScroll: true });
    });
  }

  function commitTextEditor(copyAfterCommit) {
    if (!textEditorSession) {
      return;
    }
    const session = textEditorSession;
    session.committing = true;
    flushTextDraft();
    command("commitText", { id: session.id, text: session.lastText || "" });
    destroyTextEditor(false);
    if (copyAfterCommit) {
      window.setTimeout(() => command("copy"), 0);
    }
  }

  function cancelTextEditor() {
    if (!textEditorSession) {
      return;
    }
    const session = textEditorSession;
    updateLocalAnnotationText(session.id, session.originalText);
    command("cancelText", { id: session.id, text: session.originalText });
    destroyTextEditor(false);
  }

  function destroyTextEditor(sendCommit) {
    if (!textEditorSession) {
      return;
    }
    const session = textEditorSession;
    if (sendCommit) {
      command("commitText", { id: session.id, text: session.lastText || "" });
    }
    window.clearTimeout(textDraftTimer);
    for (const unregister of session.unregister || []) {
      try {
        unregister();
      } catch (_) {}
    }
    session.editor.setRootElement(null);
    session.root.onkeydown = null;
    session.root.onblur = null;
    session.root.style.display = "none";
    session.root.textContent = "";
    textEditorSession = null;
  }

  function render() {
    markDirty();
    renderDirty();
  }

  function renderDirty() {
    const rect = toolbarRect();
    if (!rect) {
      shouldAnimateToolbarAppearance(null);
      watermarkInput.style.display = "none";
      clearAnnotationNodeCache();
      selectionLayer.destroyChildren();
      uiLayer.destroyChildren();
      previewLayer.destroyChildren();
      watermarkLayer.destroyChildren();
      dirty.ui = false;
      dirty.committed = false;
      dirty.selection = false;
      dirty.preview = false;
      committedLayer.batchDraw();
      selectionLayer.batchDraw();
      uiLayer.batchDraw();
      watermarkLayer.batchDraw();
      previewLayer.batchDraw();
      return;
    }
    if (dirty.committed) {
      if (pendingCommittedDiff) {
        renderCommittedDiff(pendingCommittedDiff);
        pendingCommittedDiff = null;
      } else {
        renderCommittedAnnotations();
      }
      renderWatermarkAnnotations();
      committedLayer.batchDraw();
      watermarkLayer.batchDraw();
    }
    if (dirty.selection) {
      renderSelection();
      selectionLayer.batchDraw();
    }
    if (dirty.preview) {
      renderPreview();
      previewLayer.batchDraw();
    }
    if (dirty.ui) {
      const animateToolbar = shouldAnimateToolbarAppearance(rect);
      sliderHotzones = [];
      colorSwatchHotzones = [];
      stopToolbarAppearTweens();
      uiLayer.destroyChildren();
      drawRegionControlBackdrop();
      drawRegionControls();
      drawRegionResizeHandles();
      drawToolbar(rect, animateToolbar);
      drawSubmenu(rect, animateToolbar);
      uiLayer.batchDraw();
    }
    syncTextEditorOverlay();
    dirty.ui = false;
    dirty.committed = false;
    dirty.selection = false;
    dirty.preview = false;
  }

  function drawToolbar(rect, animate) {
    const s = scale();
    const p = palette();
    const radius = 10 * s;
    const group = new Konva.Group({ x: rect.x, y: rect.y, name: "ui" });
    uiLayer.add(group);
    if (animate) {
      animateToolbarSurface(group);
    }

    drawThemeDrawer(group, rect, s);
    group.add(
      new Konva.Rect({
        x: 0,
        y: 0,
        width: rect.width,
        height: rect.height,
        cornerRadius: radius,
        fill: "rgba(0,0,0,0.16)",
        shadowColor: "rgba(0,0,0,0.34)",
        shadowBlur: 18 * s,
        shadowOffsetY: 7 * s,
        shadowOpacity: 0.55,
        listening: false,
      })
    );
    group.add(
      new Konva.Rect({
        x: 0,
        y: 0,
        width: rect.width,
        height: rect.height,
        cornerRadius: radius,
        fill: p.bg,
        name: "ui",
      })
    );
    drawGradientBorder(group, 0.5, 0.5, rect.width - 1, rect.height - 1, radius, p, s);

    let x = 0;
    for (const item of tools) {
      const w = item.width * s;
      let hoverEffect = null;
      if (item.divider) {
        group.add(new Konva.Line({ points: [x + w / 2, 8 * s, x + w / 2, rect.height - 8 * s], stroke: p.separator, strokeWidth: Math.max(1, 1 * s), name: "ui" }));
        x += w;
        continue;
      }

      const button = new Konva.Group({ x, y: 0, name: "ui" });
      group.add(button);
      button.add(new Konva.Rect({ x: 0, y: 0, width: w, height: rect.height, fill: "rgba(0,0,0,0)", name: "ui" }));

      const selected = item.tool && state.activeTool === item.tool;
      if (selected) {
        const selectedSize = 24 * s;
        button.add(
          new Konva.Rect({
            x: (w - selectedSize) / 2,
            y: (rect.height - selectedSize) / 2,
            width: selectedSize,
            height: selectedSize,
            cornerRadius: 6 * s,
            fill: p.selected,
            name: "ui",
          })
        );
      }

      if (item.action === "numbering") {
        drawNumberingButton(button, w, rect.height, p, s);
        button.on("click tap", () => command("toggleNumbering"));
      } else if (item.action === "grip") {
        drawIcon(button, "grip", 0, 0, w, p.icon, 1);
        button.on("mousedown touchstart", (evt) => {
          const point = pointerPosition();
          if (!point || !state.toolbar) {
            return;
          }
          const toolbar = toolbarRect();
          evt.cancelBubble = true;
          toolbarDrag = {
            dx: point.x - toolbar.x,
            dy: point.y - toolbar.y,
          };
          container.style.cursor = "grabbing";
        });
        button.on("mouseenter", () => (container.style.cursor = "grab"));
      } else {
        const iconSize = 24 * s;
        hoverEffect = drawHoverableToolbarIcon(button, item.icon, (w - iconSize) / 2, (rect.height - iconSize) / 2, iconSize, p.icon, activeColor(), selected, s);
        if (item.tool) {
          button.on("click tap", () => command("selectTool", { tool: item.tool }));
        } else {
          button.on("click tap", () => command(item.action));
        }
      }

      button.on("mouseenter", () => {
        container.style.cursor = item.action === "grip" ? "grab" : "pointer";
        if (hoverEffect) {
          hoverEffect.enter();
        }
      });
      button.on("mouseleave", () => {
        if (hoverEffect) {
          hoverEffect.leave();
        }
        updateDefaultCursor();
      });
      x += w;
    }
  }

  function drawRegionControls() {
    if (!state || !state.captureRegion || regionControlChromeHidden()) {
      return;
    }
    const region = physicalToCssRect(state.captureRegion);
    const s = scale();
    const lockIconSize = 16 * s * 1.1;
    const ratioIconSize = lockIconSize * 1.2;
    const iconSize = Math.max(lockIconSize, ratioIconSize);
    const margin = 18 * s;
    const gap = 11 * s;
    const dividerHeight = 22 * s;
    const controlsWidth = lockIconSize + ratioIconSize + gap * 2;
    const left = Math.max(6 * s, Math.min(region.x + region.width / 2 - controlsWidth / 2, window.innerWidth - controlsWidth - 6 * s));
    const top = Math.max(8 * s, Math.min(region.y + margin, window.innerHeight - iconSize - 8 * s));
    const lockName = state.regionControls && state.regionControls.locked ? "regionLocked" : "regionUnlocked";
    const ratioName = ratioIconName(state.regionControls && state.regionControls.aspectRatio);
    const normal = "#FFFFFF";
    const hover = "#FF3B30";
    const group = new Konva.Group({ x: left, y: top, name: "ui" });
    uiLayer.add(group);
    group.add(new Konva.Rect({
      x: -8 * s,
      y: -6 * s,
      width: controlsWidth + 16 * s,
      height: iconSize + 12 * s,
      fill: "rgba(0,0,0,0)",
      name: "ui",
    }));

    drawRegionControlButton(group, 0, (iconSize - lockIconSize) / 2, lockIconSize, lockName, normal, hover, () => {
      state = {
        ...state,
        regionControls: {
          ...(state.regionControls || {}),
          locked: !(state.regionControls && state.regionControls.locked),
        },
      };
      command("toggleRegionLock");
      scheduleRender(state, { ui: true });
    });

    const dividerX = lockIconSize + gap;
    group.add(new Konva.Line({
      points: [dividerX, (iconSize - dividerHeight) / 2, dividerX, (iconSize + dividerHeight) / 2],
      stroke: normal,
      opacity: 0.65,
      strokeWidth: Math.max(2, 2 * s),
      lineCap: "round",
      listening: false,
      name: "ui",
    }));

    drawRegionControlButton(group, dividerX + gap, 0, ratioIconSize, ratioName, normal, hover, () => {
      const current = state.regionControls && state.regionControls.aspectRatio;
      state = {
        ...state,
        regionControls: {
          ...(state.regionControls || {}),
          aspectRatio: nextAspectRatio(current),
        },
      };
      command("cycleAspectRatio");
      scheduleRender(state, { ui: true });
    });
  }

  function drawRegionControlButton(group, x, y, size, icon, normalColor, hoverColor, onClick) {
    const button = new Konva.Group({ x, y, name: "ui" });
    group.add(button);
    button.add(new Konva.Rect({ x: -5, y: -5, width: size + 10, height: size + 10, fill: "rgba(0,0,0,0)", name: "ui" }));
    const normalIcon = drawIcon(button, icon, 0, 0, size, normalColor, 0.9);
    const hoverIcon = drawIcon(button, icon, 0, 0, size, hoverColor, 0);
    if (hoverIcon) {
      hoverIcon.shadowColor(hoverColor);
      hoverIcon.shadowBlur(14 * scale());
      hoverIcon.shadowOpacity(0.85);
      hoverIcon.shadowForStrokeEnabled(false);
    }
    let tween = null;
    const setHover = (hovered) => {
      if (tween) {
        tween.destroy();
      }
      if (!normalIcon || !hoverIcon) {
        return;
      }
      tween = new Konva.Tween({
        node: hoverIcon,
        duration: 0.12,
        easing: Konva.Easings.EaseInOut,
        opacity: hovered ? 1 : 0,
      });
      tween.play();
      normalIcon.opacity(hovered ? 0 : 0.9);
      uiLayer.batchDraw();
    };
    button.on("mouseenter", () => {
      container.style.cursor = "pointer";
      setHover(true);
    });
    button.on("mouseleave", () => {
      updateDefaultCursor();
      setHover(false);
    });
    button.on("click tap", (evt) => {
      evt.cancelBubble = true;
      onClick();
    });
  }

  function drawRegionControlBackdrop() {
    return;
  }

  function drawRegionResizeHandles() {
    if (!state || !state.captureRegion) {
      return;
    }
    const region = physicalToCssRect(state.captureRegion);
    const s = scale();
    const size = Math.max(5, 4.5 * s);
    const half = size / 2;
    const hitPad = 13 * s;
    const handles = [
      { key: "nw", cursor: "nwse-resize", x: region.x, y: region.y },
      { key: "n", cursor: "ns-resize", x: region.x + region.width / 2, y: region.y },
      { key: "ne", cursor: "nesw-resize", x: region.x + region.width, y: region.y },
      { key: "e", cursor: "ew-resize", x: region.x + region.width, y: region.y + region.height / 2 },
      { key: "se", cursor: "nwse-resize", x: region.x + region.width, y: region.y + region.height },
      { key: "s", cursor: "ns-resize", x: region.x + region.width / 2, y: region.y + region.height },
      { key: "sw", cursor: "nesw-resize", x: region.x, y: region.y + region.height },
      { key: "w", cursor: "ew-resize", x: region.x, y: region.y + region.height / 2 },
    ];
    handles.forEach((handle) => {
      const x = Math.max(half, Math.min(window.innerWidth - half, handle.x));
      const y = Math.max(half, Math.min(window.innerHeight - half, handle.y));
      const group = new Konva.Group({ x: x - half, y: y - half, name: "ui" });
      uiLayer.add(group);
      group.add(new Konva.Rect({
        x: -hitPad,
        y: -hitPad,
        width: size + hitPad * 2,
        height: size + hitPad * 2,
        fill: "rgba(0,0,0,0)",
        name: "ui",
      }));
      group.add(new Konva.Rect({
        x: 0,
        y: 0,
        width: size,
        height: size,
        fill: "#FF3B30",
        stroke: "#FF3B30",
        strokeWidth: Math.max(1, 1.25 * s),
        cornerRadius: Math.max(1.5, 1.5 * s),
        shadowColor: "#FF3B30",
        shadowBlur: 10 * s,
        shadowOpacity: 0.58,
        name: "ui",
      }));
      group.on("mouseenter", () => {
        container.style.cursor = handle.cursor;
      });
      group.on("mouseleave", updateDefaultCursor);
      group.on("mousedown touchstart", (evt) => {
        evt.cancelBubble = true;
        captureBrowserPointer(evt);
        const point = pointerPosition() || { x, y };
        regionResizeDrag = true;
        if (state.regionControls && state.regionControls.aspectRatio !== "custom" && !isCornerRegionHandle(handle.key)) {
          state = {
            ...state,
            regionControls: {
              ...state.regionControls,
              aspectRatio: "custom",
            },
          };
          scheduleRender(state, { ui: true });
        }
        command("pointerDown", cssToPhysicalPoint(point));
      });
    });
  }

  function hasUserAnnotations() {
    return !!(state && Array.isArray(state.annotations) && state.annotations.some((annotation) => annotation && annotation.kind && annotation.kind.type !== "watermark"));
  }

  function regionControlChromeHidden() {
    return hasUserAnnotations() || !!preview;
  }

  function isCornerRegionHandle(key) {
    return key === "nw" || key === "ne" || key === "se" || key === "sw";
  }

  function ratioIconName(mode) {
    switch (mode) {
      case "9x16":
        return "ratio9x16";
      case "16x9":
        return "ratio16x9";
      case "1x1":
        return "ratio1x1";
      case "4x5":
        return "ratio4x5";
      default:
        return "ratioCustom";
    }
  }

  function nextAspectRatio(mode) {
    switch (mode) {
      case "custom":
        return "9x16";
      case "9x16":
        return "16x9";
      case "16x9":
        return "1x1";
      case "1x1":
        return "4x5";
      default:
        return "custom";
    }
  }

  function drawThemeDrawer(toolbarGroup, rect, s) {
    const overlap = 20 * s;
    const restVisible = 3 * s;
    const openVisible = 26 * s;
    const openOvershoot = 2 * s;
    const width = overlap + openVisible;
    const height = Math.max(1, rect.height - 4 * s);
    const y = (rect.height - height) / 2;
    const openX = rect.width - overlap;
    const restX = openX - (openVisible - restVisible);
    const drawerTheme = state.theme === "dark" ? "light" : "dark";
    const targetPalette = paletteFor(drawerTheme);
    const targetIcon = drawerTheme === "dark" ? "darkMode" : "lightMode";
    const iconColor = drawerTheme === "dark" ? "#B3B3B3" : "#4D4D4D";
    const iconSize = 16.8 * s;
    const iconX = overlap + (openVisible - iconSize) / 2;
    const iconY = (height - iconSize) / 2;
    const iconHiddenY = height + 2 * s;
    const group = new Konva.Group({
      x: themeDrawer.open ? openX : restX,
      y,
      name: "ui",
      clipFunc: (ctx) => {
        drawerTabPath(ctx, 0, 0, width, height);
      },
    });
    toolbarGroup.add(group);
    themeDrawer.node = group;

    drawDrawerSurface(group, width, height, targetPalette, drawerTheme, s);
    const revealAfterToggle = themeDrawer.revealAfterToggle;
    themeDrawer.icon = drawIcon(group, targetIcon, iconX, themeDrawer.open && !revealAfterToggle ? iconY : iconHiddenY, iconSize, iconColor, themeDrawer.open && !revealAfterToggle ? 1 : 0);

    const hit = new Konva.Rect({
      x: rect.width,
      y,
      width: openVisible,
      height,
      fill: "rgba(0,0,0,0)",
      name: "ui",
    });
    toolbarGroup.add(hit);
    hit.on("mouseenter", () => {
      container.style.cursor = "pointer";
      animateThemeDrawer(true, false, { openX, restX, overshoot: openOvershoot, iconY, iconHiddenY });
    });
    hit.on("mouseleave", () => {
      updateDefaultCursor();
      if (!themeDrawer.clicking) {
        animateThemeDrawer(false, false, { openX, restX, overshoot: openOvershoot, iconY, iconHiddenY });
      }
    });
    hit.on("click tap", (evt) => {
      evt.cancelBubble = true;
      if (themeDrawer.clicking) {
        return;
      }
      themeDrawer.clicking = true;
      animateThemeDrawer(false, true, { openX, restX, overshoot: openOvershoot, iconY, iconHiddenY }, () => {
        themeDrawer.open = false;
        themeDrawer.clicking = false;
        command("toggleTheme");
      });
    });
  }

  function animateThemeDrawer(open, click, geometry, onDone) {
    if (!themeDrawer.node) {
      return;
    }
    if (themeDrawer.tween) {
      themeDrawer.tween.destroy();
      themeDrawer.tween = null;
    }
    if (themeDrawer.settleTween) {
      themeDrawer.settleTween.destroy();
      themeDrawer.settleTween = null;
    }
    if (themeDrawer.iconTween) {
      themeDrawer.iconTween.destroy();
      themeDrawer.iconTween = null;
    }
    themeDrawer.pendingDone = onDone || null;
    themeDrawer.open = open;
    const node = themeDrawer.node;
    const targetX = open ? geometry.openX : geometry.restX;
    const overshootX = open ? geometry.openX + geometry.overshoot : targetX;
    if (!open) {
      if (click) {
        themeDrawer.tween = new Konva.Tween({
          node,
          duration: 0.08,
          easing: Konva.Easings.EaseInOut,
          x: geometry.openX + geometry.overshoot,
          onFinish: () => {
            themeDrawer.settleTween = new Konva.Tween({
              node,
              duration: 0.24,
              easing: Konva.Easings.EaseInOut,
              x: targetX,
              onFinish: () => {
                const done = themeDrawer.pendingDone;
                themeDrawer.pendingDone = null;
                if (done) done();
              },
            });
            themeDrawer.settleTween.play();
          },
        });
        themeDrawer.tween.play();
        return;
      }
      const closeDrawer = () => {
        themeDrawer.tween = new Konva.Tween({
          node,
          duration: 0.24,
          easing: Konva.Easings.EaseInOut,
          x: targetX,
          onFinish: () => {
            const done = themeDrawer.pendingDone;
            themeDrawer.pendingDone = null;
            if (done) done();
          },
        });
        themeDrawer.tween.play();
      };
      animateThemeIcon(false, geometry, click ? 0.22 : 0.18, closeDrawer);
      return;
    }
    const duration = click ? 0.34 : 0.32;
    themeDrawer.tween = new Konva.Tween({
      node,
      duration,
      easing: Konva.Easings.EaseInOut,
      x: overshootX,
      onFinish: () => {
        if (!open) {
          if (onDone) onDone();
          return;
        }
        themeDrawer.settleTween = new Konva.Tween({
          node,
          duration: click ? 0.1 : 0.09,
          easing: Konva.Easings.EaseInOut,
          x: targetX,
          onFinish: () => {
            const done = themeDrawer.pendingDone;
            themeDrawer.pendingDone = null;
            if (done) done();
          },
        });
        themeDrawer.settleTween.play();
      },
    });
    themeDrawer.tween.play();
    animateThemeIcon(true, geometry, click ? 0.26 : 0.22);
  }

  function animateThemeIcon(show, geometry, duration, onDone) {
    if (themeDrawer.iconTween) {
      themeDrawer.iconTween.destroy();
      themeDrawer.iconTween = null;
    }
    if (!themeDrawer.icon) {
      if (onDone) onDone();
      return;
    }
    themeDrawer.iconTween = new Konva.Tween({
      node: themeDrawer.icon,
      duration,
      easing: Konva.Easings.EaseInOut,
      y: show ? geometry.iconY : geometry.iconHiddenY,
      opacity: show ? 1 : 0,
      onFinish: () => {
        if (onDone) onDone();
      },
    });
    themeDrawer.iconTween.play();
  }

  function drawerTabPath(ctx, x, y, width, height) {
    const topInset = height * 0.06;
    const bottomInset = height * 0.06;
    const right = x + width;
    const bottom = y + height;
    ctx.beginPath();
    ctx.moveTo(x, y + topInset);
    ctx.bezierCurveTo(x + width * 0.22, y - height * 0.04, x + width * 0.68, y - height * 0.03, right - height * 0.20, y + height * 0.10);
    ctx.bezierCurveTo(right - height * 0.02, y + height * 0.20, right, y + height * 0.32, right, y + height * 0.50);
    ctx.bezierCurveTo(right, y + height * 0.68, right - height * 0.02, y + height * 0.80, right - height * 0.20, y + height * 0.90);
    ctx.bezierCurveTo(x + width * 0.68, bottom + height * 0.03, x + width * 0.22, bottom + height * 0.04, x, bottom - bottomInset);
    ctx.lineTo(x, y + topInset);
    ctx.closePath();
  }

  function drawDrawerSurface(group, width, height, p, theme, s) {
    group.add(
      new Konva.Shape({
        listening: false,
        name: "ui",
        sceneFunc: (ctx) => {
          const fill = ctx.createLinearGradient(0, 0, width, 0);
          if (theme === "dark") {
            fill.addColorStop(0, "#050505");
            fill.addColorStop(0.28, "#111111");
            fill.addColorStop(0.74, p.bg);
            fill.addColorStop(1, "#202020");
          } else {
            fill.addColorStop(0, "#FFFFFF");
            fill.addColorStop(0.35, p.bg);
            fill.addColorStop(0.78, "#F7F7F7");
            fill.addColorStop(1, "#E5E5E5");
          }
          drawerTabPath(ctx, 0.5, 0.5, width - 1, height - 1);
          ctx.fillStyle = fill;
          ctx.fill();

          const stroke = ctx.createLinearGradient(0, 0, width, 0);
          if (theme === "dark") {
            stroke.addColorStop(0, "rgba(0,0,0,0.96)");
            stroke.addColorStop(0.52, "rgba(255,255,255,0.10)");
            stroke.addColorStop(1, "rgba(0,0,0,0.80)");
          } else {
            stroke.addColorStop(0, "rgba(255,255,255,0.95)");
            stroke.addColorStop(0.52, "rgba(255,255,255,0.65)");
            stroke.addColorStop(1, "rgba(180,180,180,0.70)");
          }
          drawerTabPath(ctx, 0.5, 0.5, width - 1, height - 1);
          ctx.strokeStyle = stroke;
          ctx.lineWidth = Math.max(1, 1 * s);
          ctx.stroke();
        },
      })
    );
  }

  function drawGradientBorder(group, x, y, width, height, radius, p, s, themeOverride) {
    const theme = themeOverride || (state && state.theme) || "light";
    group.add(
      new Konva.Shape({
        x: 0,
        y: 0,
        listening: false,
        name: "ui",
        sceneFunc: (ctx) => {
          const gradient = ctx.createLinearGradient(0, y, 0, y + height);
          gradient.addColorStop(0, p.borderTop);
          gradient.addColorStop(0.48, theme === "dark" ? "rgba(255,255,255,0.055)" : "rgba(255,255,255,0.56)");
          gradient.addColorStop(0.52, theme === "dark" ? "rgba(0,0,0,0.22)" : "rgba(210,210,210,0.42)");
          gradient.addColorStop(1, p.borderBottom);
          roundedRectPath(ctx, x, y, width, height, radius);
          ctx.strokeStyle = gradient;
          ctx.lineWidth = Math.max(1, 1 * s);
          ctx.stroke();
        },
      })
    );
  }

  function drawNumberingButton(group, w, h, p, s) {
    const iconSize = 22 * s;
    const iconAlpha = state.numberingEnabled ? 1 : 0.28;
    drawIcon(group, "numbering", (w - iconSize) / 2, (h - iconSize) / 2, iconSize, p.icon, iconAlpha);

    const trackW = 11 * s;
    const trackH = 1.05 * s;
    const trackX = (w - trackW) / 2;
    const trackY = h - 7 * s;
    const knobR = 1.9 * s;
    const on = !!state.numberingEnabled;
    group.add(
      new Konva.Line({
        points: [trackX, trackY, trackX + trackW, trackY],
        stroke: on ? "#6BC565" : p.slider,
        strokeWidth: trackH,
        lineCap: "round",
        name: "ui",
      })
    );
    const knob = new Konva.Circle({
      x: trackX + (on ? trackW : 0),
      y: trackY,
      radius: knobR,
      fill: on ? "#6BC565" : p.slider,
      name: "ui",
    });
    group.add(knob);

    if (lastNumberingEnabled !== on) {
      lastNumberingEnabled = on;
      if (toggleTween) {
        toggleTween.destroy();
      }
      knob.x(trackX + (on ? 0 : trackW));
      toggleTween = new Konva.Tween({
        node: knob,
        duration: 0.09,
        easing: Konva.Easings.EaseOut,
        x: trackX + (on ? trackW : 0),
      });
      toggleTween.play();
    }
  }

  function drawSubmenu(toolbar, animate) {
    const tool = state.activeSubmenu;
    if (!tool || !state.captureRegion) {
      watermarkInput.style.display = "none";
      return;
    }
    const s = scale();
    const p = palette();
    const layout = submenuLayout(toolbar, tool, s);
    const group = new Konva.Group({ x: layout.x, y: layout.y, name: "ui" });
    uiLayer.add(group);
    if (animate) {
      animateToolbarSurface(group);
    }
    const radius = 8 * s;
    group.add(new Konva.Rect({ x: 0, y: 0, width: layout.width, height: layout.height, cornerRadius: radius, fill: "rgba(0,0,0,0.14)", shadowColor: "rgba(0,0,0,0.28)", shadowBlur: 14 * s, shadowOffsetY: 5 * s, shadowOpacity: 0.45, listening: false }));
    group.add(new Konva.Rect({ x: 0, y: 0, width: layout.width, height: layout.height, cornerRadius: radius, fill: p.submenuBg, name: "ui" }));
    group.add(new Konva.RegularPolygon({ x: layout.notchX - layout.x, y: 0, sides: 3, radius: 9 * s, rotation: 0, fill: p.submenuBg, name: "ui" }));
    group.add(new Konva.Line({ points: [radius, layout.height - 0.5, layout.width - radius, layout.height - 0.5], stroke: p.borderBottom, strokeWidth: 1, name: "ui" }));

    let x = 8 * s;
    if (["rectangle", "oval", "line", "arrow"].includes(tool)) {
      x = drawSlider(group, x, 0, "stroke", 1, 24, currentWidth(), p, s);
      x = drawDivider(group, x, p, s);
      x = drawColors(group, x, p, s);
    } else if (tool === "step") {
      x = drawOptionIcon(group, x, "restartNumbering", false, () => command("setNumberingMode", { mode: "restart" }), p, s);
      x = drawOptionIcon(group, x, "continueNumbering", false, () => command("setNumberingMode", { mode: "continue" }), p, s);
    } else if (tool === "pen") {
      x = drawOptionIcon(group, x, "miniLine", state.penMode === "free", () => command("setPenMode", { mode: "free" }), p, s);
      x = drawOptionIcon(group, x, "miniArrow", state.penMode === "arrow", () => command("setPenMode", { mode: "arrow" }), p, s);
      x = drawDivider(group, x, p, s);
      x = drawSlider(group, x, 0, "stroke", 1, 24, currentWidth(), p, s);
      x = drawDivider(group, x, p, s);
      x = drawColors(group, x, p, s);
    } else if (tool === "highlighter") {
      x = drawOptionIcon(group, x, "miniLine", state.highlighterShape === "line", () => command("setHighlighterShape", { shape: "line" }), p, s);
      x = drawOptionIcon(group, x, "area", state.highlighterShape === "area", () => command("setHighlighterShape", { shape: "area" }), p, s);
      x = drawDivider(group, x, p, s);
      x = drawSlider(group, x, 0, "stroke", 1, 24, currentWidth(), p, s);
      x = drawDivider(group, x, p, s);
      x = drawColors(group, x, p, s);
    } else if (tool === "text") {
      const selectedText = selectedAnnotation();
      const textFilled = selectedText && selectedText.kind && selectedText.kind.type === "text" ? !!selectedText.kind.filled : !!state.textFilled;
      x = drawOptionIcon(group, x, "lineText", !textFilled, () => {
        applyLocalTextFilled(false);
        command("setTextFilled", { filled: false });
      }, p, s);
      x = drawOptionIcon(group, x, "solidText", textFilled, () => {
        applyLocalTextFilled(true);
        command("setTextFilled", { filled: true });
      }, p, s);
      x = drawDivider(group, x, p, s);
      x = drawSlider(group, x, 0, "font", 27, 56, currentFontSize(), p, s);
      x = drawDivider(group, x, p, s);
      x = drawColors(group, x, p, s);
    } else if (tool === "tag") {
      x = drawSlider(group, x, 0, "stroke", 6, 24, Math.max(6, currentWidth()), p, s);
      x = drawDivider(group, x, p, s);
      x = drawSlider(group, x, 0, "font", 27, 56, currentFontSize(), p, s);
      x = drawDivider(group, x, p, s);
      x = drawColors(group, x, p, s);
    } else if (tool === "watermark") {
      x = drawOptionIcon(group, x, "calendar", !!state.watermarkDateEnabled, () => command("setWatermarkMode", { mode: "date" }), p, s);
      x = drawOptionIcon(group, x, "image", state.watermarkMode === "image", () => command("setWatermarkMode", { mode: "image" }), p, s);
      x = drawOptionIcon(group, x, "cancel", false, () => {
        watermarkInput.value = "";
        watermarkInput.blur();
        command("clearWatermark");
      }, p, s);
      x = drawDivider(group, x, p, s);
      x = drawSlider(group, x, 0, "font", 27, 56, currentFontSize(), p, s);
      positionWatermarkInput(layout.x + x, layout.y + 3 * s, 122 * s, 18 * s);
      x += 132 * s;
      x = drawColors(group, x, p, s);
    } else {
      watermarkInput.style.display = "none";
    }
    group.on("mousedown touchstart", (evt) => {
      const point = stagePointFromKonvaEvent(evt);
      const color = colorAtSwatchPoint(point);
      evt.cancelBubble = true;
      if (evt.evt && evt.evt.preventDefault) {
        evt.evt.preventDefault();
      }
      if (!color) {
        return;
      }
      if (state.activeSubmenu !== "watermark") {
        applyLocalColor(color);
      }
      command("setColor", { color, ...cssToPhysicalPoint(point) });
    });
  }

  function submenuLayout(toolbar, tool, s) {
    const widths = {
      rectangle: 250,
      oval: 250,
      line: 250,
      arrow: 250,
      pen: 304,
      highlighter: 304,
      text: 326,
      tag: 374,
      watermark: 440,
      mosaic: 250,
      step: 72,
    };
    const width = (widths[tool] || 220) * s;
    const height = 24 * s;
    const anchor = toolbarButtonCenter(toolbar, tool);
    const margin = 8 * s;
    const x = Math.max(margin, Math.min(window.innerWidth - width - margin, anchor - width / 2));
    const y = toolbar.y + toolbar.height + 26 * s;
    return { x, y, width, height, notchX: anchor };
  }

  function toolbarButtonCenter(toolbar, tool) {
    let x = toolbar.x;
    for (const item of tools) {
      const w = item.width * scale();
      if (item.tool === tool || ((tool === "numbering" || tool === "step") && item.action === "numbering")) {
        return x + w / 2;
      }
      x += w;
    }
    return toolbar.x + toolbar.width / 2;
  }

  function drawDivider(group, x, p, s) {
    group.add(new Konva.Line({ points: [x + 4 * s, 4 * s, x + 4 * s, 20 * s], stroke: p.separator, strokeWidth: Math.max(1, s), name: "ui" }));
    return x + 10 * s;
  }

  function drawOptionIcon(group, x, icon, selected, onClick, p, s) {
    const box = 24 * s;
    const item = new Konva.Group({ x, y: 0, name: "ui" });
    group.add(item);
    item.add(new Konva.Rect({ x: 0, y: 0, width: box, height: box, cornerRadius: 4 * s, fill: selected ? p.selected : "rgba(0,0,0,0)", name: "ui" }));
    drawIcon(item, icon, 4 * s, 4 * s, 16 * s, p.icon, selected ? 1 : 0.92);
    item.on("mousedown touchstart", (evt) => {
      evt.cancelBubble = true;
      if (evt.evt && evt.evt.preventDefault) {
        evt.evt.preventDefault();
      }
      onClick();
    });
    item.on("mouseenter", () => (container.style.cursor = "pointer"));
    item.on("mouseleave", updateDefaultCursor);
    return x + 28 * s;
  }

  function drawColors(group, x, p, s) {
    for (const color of colors) {
      const selected = activeColor() === color;
      const box = 16 * s;
      const hit = new Konva.Group({ x, y: 4 * s, name: "ui" });
      group.add(hit);
      const swatch = new Konva.Rect({ x: 0, y: 0, width: box, height: box, cornerRadius: 3 * s, fill: color, stroke: selected ? p.swatchBorder : null, strokeWidth: selected ? 3 : 0, name: "ui" });
      hit.add(swatch);
      const swatchOrigin = swatch.getAbsolutePosition();
      colorSwatchHotzones.push({ color, rect: { x: swatchOrigin.x, y: swatchOrigin.y, width: box, height: box } });
      swatch.on("mouseenter", () => (container.style.cursor = "pointer"));
      swatch.on("mouseleave", updateDefaultCursor);
      x += 21 * s;
    }
    watermarkInput.style.display = state.activeSubmenu === "watermark" ? watermarkInput.style.display : "none";
    return x + 2 * s;
  }

  function drawSlider(group, x, y, kind, min, max, value, p, s) {
    const sliderInfo = { kind, min, max };
    const width = (kind === "font" ? 118 : 112) * s;
    const centerY = y + 12 * s;
    const startX = x + (kind === "font" ? 25 : 16) * s;
    const endX = x + width - (kind === "font" ? 25 : 16) * s;
    const t = sliderPosition(kind, min, max, value);
    const handleX = startX + (endX - startX) * t;

    if (kind === "font") {
      drawIcon(group, "smallerFont", x + 2 * s, y + 4 * s, 16 * s, p.slider, 1);
      drawIcon(group, "largerFont", x + width - 18 * s, y + 3 * s, 18 * s, p.slider, 1);
    } else {
      group.add(new Konva.Circle({ x: x + 7 * s, y: centerY, radius: 2.1 * s, fill: p.slider, name: "ui" }));
      group.add(new Konva.Circle({ x: x + width - 7 * s, y: centerY, radius: 4.8 * s, fill: p.slider, name: "ui" }));
    }
    group.add(new Konva.Line({ points: [startX, centerY, endX, centerY], stroke: p.slider, strokeWidth: 2.2 * s, lineCap: "round", name: "ui" }));
    group.add(new Konva.Circle({ x: handleX, y: centerY, radius: 6.4 * s, fill: "#FFFFFF", stroke: "#CFCFCF", strokeWidth: 2.6 * s, name: "ui" }));

    const hit = new Konva.Rect({ x, y, width, height: 24 * s, fill: "rgba(0,0,0,0)", name: "ui" });
    group.add(hit);
    const absolute = group.getAbsolutePosition();
    sliderHotzones.push({
      slider: sliderInfo,
      rect: { x: absolute.x + x, y: absolute.y + y, width, height: 24 * s },
    });
    const apply = () => {
      if (kind === "font" && state && state.activeSubmenu === "watermark" && document.activeElement === watermarkInput) {
        watermarkInput.blur();
        command("blurWatermarkText");
      }
      const pointer = stage.getPointerPosition();
      if (!pointer) {
        return;
      }
      const localX = pointer.x - group.x();
      const nextT = Math.max(0, Math.min(1, (localX - startX) / (endX - startX)));
      command(kind === "font" ? "setFontSize" : "setStrokeWidth", { value: sliderValue(kind, min, max, nextT) });
    };
    hit.on("mousedown touchstart", (evt) => {
      evt.cancelBubble = true;
      apply();
      stage.on("mousemove.slider touchmove.slider", apply);
      stage.on("mouseup.slider touchend.slider", () => {
        stage.off(".slider");
      });
    });
    hit.on("wheel", (evt) => {
      evt.cancelBubble = true;
      adjustSliderFromWheel(sliderInfo, evt.evt);
    });
    hit.on("mouseenter", () => {
      hoveredSlider = sliderInfo;
      container.style.cursor = "pointer";
    });
    hit.on("mouseleave", () => {
      if (
        hoveredSlider &&
        hoveredSlider.kind === sliderInfo.kind &&
        hoveredSlider.min === sliderInfo.min &&
        hoveredSlider.max === sliderInfo.max
      ) {
        hoveredSlider = null;
      }
      updateDefaultCursor();
    });
    return x + width + 4 * s;
  }

  function sliderPosition(kind, min, max, value) {
    const raw = Math.max(0, Math.min(1, (value - min) / Math.max(1, max - min)));
    if (kind === "font") {
      return 0.3 + raw * 0.7;
    }
    return raw;
  }

  function sliderValue(kind, min, max, t) {
    if (kind === "font") {
      return min + (max - min) * Math.max(0, (t - 0.3) / 0.7);
    }
    return min + (max - min) * t;
  }

  function positionWatermarkInput(x, y, w, h) {
    watermarkInput.style.display = "block";
    watermarkInput.style.left = `${x}px`;
    watermarkInput.style.top = `${y}px`;
    watermarkInput.style.width = `${w}px`;
    watermarkInput.style.height = `${h}px`;
    watermarkInput.style.fontSize = `${Math.max(12, 11 * scale())}px`;
    if (document.activeElement !== watermarkInput) {
      watermarkInput.value = state.watermarkText || "";
    }
    if (state.editingWatermarkText && document.activeElement !== watermarkInput) {
      requestAnimationFrame(() => {
        watermarkInput.focus();
        watermarkInput.setSelectionRange(watermarkInput.value.length, watermarkInput.value.length);
      });
    }
  }

  watermarkInput.addEventListener("focus", () => command("focusWatermarkText"));
  watermarkInput.addEventListener("input", () => command("setWatermarkText", { text: watermarkInput.value }));
  watermarkInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && !event.shiftKey) {
      watermarkInput.blur();
      command("blurWatermarkText");
      command("copy");
      event.preventDefault();
    } else if (event.key === "Escape") {
      watermarkInput.blur();
      command("blurWatermarkText");
      event.preventDefault();
    }
  });

  function updateDefaultCursor() {
    container.style.cursor = state && state.captureRegion ? "crosshair" : "crosshair";
  }

  function pointerPosition() {
    const p = stage.getPointerPosition();
    return p ? { x: p.x, y: p.y } : null;
  }

  function captureBrowserPointer(evt) {
    const nativeEvent = evt && evt.evt;
    if (
      nativeEvent &&
      nativeEvent.pointerId != null &&
      container.setPointerCapture &&
      !container.hasPointerCapture(nativeEvent.pointerId)
    ) {
      try {
        container.setPointerCapture(nativeEvent.pointerId);
      } catch (_) {}
    }
  }

  function releaseBrowserPointer(evt) {
    const nativeEvent = evt && evt.evt;
    if (
      nativeEvent &&
      nativeEvent.pointerId != null &&
      container.releasePointerCapture &&
      container.hasPointerCapture(nativeEvent.pointerId)
    ) {
      try {
        container.releasePointerCapture(nativeEvent.pointerId);
      } catch (_) {}
    }
  }

  function pointInRect(point, rect, pad) {
    return (
      rect &&
      point.x >= rect.x - pad &&
      point.x <= rect.x + rect.width + pad &&
      point.y >= rect.y - pad &&
      point.y <= rect.y + rect.height + pad
    );
  }

  function distanceToSegment(point, a, b) {
    const dx = b.x - a.x;
    const dy = b.y - a.y;
    const lenSq = dx * dx + dy * dy;
    if (lenSq <= 0.0001) {
      return Math.hypot(point.x - a.x, point.y - a.y);
    }
    const t = Math.max(0, Math.min(1, ((point.x - a.x) * dx + (point.y - a.y) * dy) / lenSq));
    return Math.hypot(point.x - (a.x + dx * t), point.y - (a.y + dy * t));
  }

  function annotationHitTest(point) {
    if (!state || !Array.isArray(state.annotations)) {
      return false;
    }
    for (let i = state.annotations.length - 1; i >= 0; i--) {
      if (annotationContainsPoint(state.annotations[i], point)) {
        return true;
      }
    }
    return false;
  }

  function annotationContainsPoint(annotation, point) {
    const kind = annotation.kind || {};
    const bounds = physicalToCssRect(annotation.bounds);
    const width = strokeWidth(annotation);
    const radius = Math.max(8, width / 2 + 8);
    if (kind.type === "rectangle") {
      return rectStrokeHit(bounds, point, radius);
    }
    if (kind.type === "oval") {
      return ovalStrokeHit(bounds, point, radius);
    }
    if (kind.type === "line" || kind.type === "arrow" || (kind.type === "highlighter" && kind.shape === "line")) {
      return distanceToSegment(point, physicalToCssPoint(kind.start), physicalToCssPoint(kind.end)) <= radius;
    }
    if (kind.type === "pen" || kind.type === "penArrow") {
      const pts = (kind.points || []).map(physicalToCssPoint);
      for (let j = 1; j < pts.length; j += 1) {
        if (distanceToSegment(point, pts[j - 1], pts[j]) <= radius) {
          return true;
        }
      }
      return false;
    }
    if (kind.type === "tag") {
      const anchor = physicalToCssPoint(kind.anchor);
      return pointInRect(point, bounds, 0) || rectStrokeHit(bounds, point, Math.max(radius, 12 * scale())) || Math.hypot(point.x - anchor.x, point.y - anchor.y) <= 14 * scale() || tagPointerHit(bounds, anchor, point);
    }
    if (kind.type === "text" && !kind.framed) {
      return pointInRect(point, inlineTextHitBounds(bounds, kind), 0);
    }
    return pointInRect(point, bounds, 4 * scale());
  }

  function rectStrokeHit(rect, point, radius) {
    return pointInRect(point, rect, radius) && !pointInRect(point, insetRect(rect, radius), 0);
  }

  function insetRect(rect, amount) {
    return {
      x: rect.x + amount,
      y: rect.y + amount,
      width: Math.max(0, rect.width - amount * 2),
      height: Math.max(0, rect.height - amount * 2),
    };
  }

  function ovalStrokeHit(rect, point, radius) {
    if (!pointInRect(point, rect, radius)) {
      return false;
    }
    const rx = Math.max(1, rect.width / 2);
    const ry = Math.max(1, rect.height / 2);
    const cx = rect.x + rect.width / 2;
    const cy = rect.y + rect.height / 2;
    const normalized = Math.sqrt(((point.x - cx) / rx) ** 2 + ((point.y - cy) / ry) ** 2);
    const tolerance = Math.max(0.05, radius / Math.min(rx, ry));
    return Math.abs(normalized - 1) <= tolerance;
  }

  function tagPointerHit(bounds, anchor, point) {
    const target = {
      x: Math.max(bounds.x, Math.min(bounds.x + bounds.width, anchor.x)),
      y: Math.max(bounds.y, Math.min(bounds.y + bounds.height, anchor.y)),
    };
    return distanceToSegment(point, anchor, target) <= 16 * scale();
  }

  function inlineTextHitBounds(bounds, kind) {
    const fontSize = physicalScalar(kind.fontSize || state.fontSize || 27);
    const padding = physicalScalar(8);
    const lines = String(kind.text || "").split(/\r?\n/);
    const width = Math.max(fontSize, lines.reduce((max, line) => Math.max(max, line.length * fontSize * 0.68), 0));
    const lineCount = Math.max(1, lines.length);
    return {
      x: bounds.x - padding,
      y: bounds.y - fontSize * 1.12 - padding,
      width: width + padding * 2,
      height: fontSize * 1.22 * lineCount + padding * 2,
    };
  }

  function regionControlHitTest(point) {
    if (!state || !state.captureRegion) {
      return false;
    }
    const region = physicalToCssRect(state.captureRegion);
    const hit = 10 * scale();
    const onFrame =
      pointInRect(point, region, 0) &&
      (point.x <= region.x + hit ||
        point.x >= region.x + region.width - hit ||
        point.y <= region.y + hit ||
        point.y >= region.y + region.height - hit);
    return onFrame;
  }

  function shouldStartPreview(point) {
    return state && state.captureRegion && !annotationHitTest(point) && !regionControlHitTest(point) && stepBadgeHitTest(point) == null;
  }

  function constrainedPreviewPoint(start, point, evt) {
    if (state && state.activeTool === "highlighter" && state.highlighterShape === "line" && evt && evt.shiftKey) {
      const dx = point.x - start.x;
      const dy = point.y - start.y;
      return Math.abs(dx) >= Math.abs(dy) ? { x: point.x, y: start.y } : { x: start.x, y: point.y };
    }
    return point;
  }

  stage.on("mousedown touchstart", (evt) => {
    if (isUiTarget(evt.target)) {
      return;
    }
    if (evt.evt && evt.evt.button === 2) {
      evt.evt.preventDefault();
      preview = null;
      if (textEditorSession) {
        commitTextEditor(false);
      } else {
        cancelTextEditor();
      }
      applyLocalDeselect();
      command("deselect");
      schedulePreviewRender();
      return;
    }
    if (textEditorSession) {
      commitTextEditor(false);
    }
    captureBrowserPointer(evt);
    watermarkInput.blur();
    const point = pointerPosition();
    if (!point) {
      return;
    }
    showCaretNow();
    if (stepBadgeHitTest(point) != null) {
      return;
    }
    preview = shouldStartPreview(point) ? { start: point, current: point, points: [point] } : null;
    if (preview && state) {
      scheduleRender(state, { ui: true });
    }
    const payload = cssToPhysicalPoint(point);
    const caretIndex = textCaretIndexAtPoint(point);
    if (caretIndex != null) {
      payload.caretIndex = caretIndex;
    }
    command("pointerDown", payload);
    schedulePreviewRender();
  });

  stage.on("mousemove touchmove", (evt) => {
    if (toolbarDrag) {
      const point = pointerPosition();
      if (point) {
        command("setToolbarOrigin", cssToPhysicalPoint({ x: point.x - toolbarDrag.dx, y: point.y - toolbarDrag.dy }));
      }
      return;
    }
    if (regionResizeDrag) {
      const point = pointerPosition();
      if (point) {
        schedulePointerMove(point);
      }
      return;
    }
    if (isUiTarget(evt.target)) {
      return;
    }
    const point = pointerPosition();
    if (!point) {
      return;
    }
    if (preview) {
      const current = constrainedPreviewPoint(preview.start, point, evt.evt);
      preview.current = current;
      if (state.activeTool === "pen" || state.activeTool === "mosaic") {
        const last = preview.points[preview.points.length - 1];
        if (!last || Math.hypot(current.x - last.x, current.y - last.y) >= 2.5 * scale()) {
          preview.points.push(current);
        }
      }
      schedulePreviewRender();
    }
    schedulePointerMove(point);
  });

  stage.on("mouseup touchend", (evt) => {
    releaseBrowserPointer(evt);
    if (regionResizeDrag) {
      regionResizeDrag = false;
      const point = pointerPosition();
      if (point) {
        flushPointerMove();
        command("pointerUp", cssToPhysicalPoint(point));
      }
      updateDefaultCursor();
      return;
    }
    if (toolbarDrag) {
      toolbarDrag = null;
      updateDefaultCursor();
      return;
    }
    if (isUiTarget(evt.target)) {
      return;
    }
    const point = pointerPosition();
    if (!point) {
      return;
    }
    flushPointerMove();
    command("pointerUp", cssToPhysicalPoint(point));
    preview = null;
    if (state) {
      scheduleRender(state, { ui: true });
    }
    schedulePreviewRender();
  });

  stage.on("dblclick dbltap", (evt) => {
    if (isUiTarget(evt.target)) {
      return;
    }
    const id = stepBadgeHitTest(pointerPosition());
    if (id != null) {
      evt.cancelBubble = true;
      command("editStepNumber", { id });
    }
  });

  function renderCommittedAnnotations() {
    if (!state || !state.captureRegion || !Array.isArray(state.annotations)) {
      clearAnnotationNodeCache();
      selectionLayer.destroyChildren();
      return;
    }
    const orderedAnnotations = nonWatermarkAnnotations(state.annotations);
    const seen = new Set();
    orderedAnnotations.forEach((annotation) => {
      seen.add(annotation.id);
      upsertCommittedAnnotation(annotation);
    });
    for (const [id, cached] of annotationNodeCache) {
      if (!seen.has(id)) {
        cached.group.destroy();
        annotationNodeCache.delete(id);
      }
    }
    syncCommittedZOrder(orderedAnnotations);
  }

  function renderCommittedDiff(diff) {
    if (!state || !state.captureRegion || !Array.isArray(state.annotations)) {
      clearAnnotationNodeCache();
      selectionLayer.destroyChildren();
      return;
    }
    for (const id of diff.removed) {
      const cached = annotationNodeCache.get(id);
      if (cached) {
        cached.group.destroy();
        annotationNodeCache.delete(id);
      }
    }
    const annotationsById = new Map(state.annotations.map((annotation) => [annotation.id, annotation]));
    for (const id of diff.changed) {
      const annotation = annotationsById.get(id);
      if (annotation && !isWatermarkAnnotation(annotation)) {
        upsertCommittedAnnotation(annotation);
      } else if (annotation) {
        const cached = annotationNodeCache.get(id);
        if (cached) {
          cached.group.destroy();
          annotationNodeCache.delete(id);
        }
      }
    }
    syncCommittedZOrder(nonWatermarkAnnotations(state.annotations));
  }

  function upsertCommittedAnnotation(annotation) {
    const signature = annotationRenderSignature(annotation);
    let cached = annotationNodeCache.get(annotation.id);
    if (!cached || cached.signature !== signature) {
      if (cached) {
        cached.group.destroy();
      }
      const group = new Konva.Group({ listening: false, name: `annotation-${annotation.id}` });
      const previousTarget = committedTarget;
      committedTarget = group;
      drawCommittedAnnotation(annotation);
      if (annotation.stepNumber != null) {
        drawStepBadge(autoStepBadgeCenter(annotation), annotation.stepNumber, annotation.id, strokeColor(annotation));
      }
      committedTarget = previousTarget;
      committedLayer.add(group);
      cached = { signature, group };
      annotationNodeCache.set(annotation.id, cached);
    }
    return cached;
  }

  function syncCommittedZOrder(orderedAnnotations) {
    orderedAnnotations.forEach((annotation, index) => {
      const cached = annotationNodeCache.get(annotation.id);
      if (cached) {
        cached.group.zIndex(index);
      }
    });
  }

  function renderWatermarkAnnotations() {
    watermarkLayer.destroyChildren();
  }

  function isWatermarkAnnotation(annotation) {
    return !!(annotation && annotation.kind && annotation.kind.type === "watermark");
  }

  function nonWatermarkAnnotations(annotations) {
    return (annotations || []).filter((annotation) => !isWatermarkAnnotation(annotation));
  }

  function renderSelection() {
    selectionLayer.destroyChildren();
    const selected = selectedAnnotation();
    if (selected) {
      drawSelection(selected);
    }
  }

  function clearAnnotationNodeCache() {
    for (const cached of annotationNodeCache.values()) {
      cached.group.destroy();
    }
    annotationNodeCache.clear();
    committedLayer.destroyChildren();
    watermarkLayer.destroyChildren();
  }

  function annotationRenderSignature(annotation) {
    return JSON.stringify({
      annotation,
      scale: scale(),
      viewportScale: viewportScale(),
      renderStyle: renderStyle(),
      editingTextId: state && state.editingTextId,
      editingStepNumberId: state && state.editingStepNumberId,
          });
  }

  function addCommitted(node) {
    committedTarget.add(node);
    return node;
  }

  function drawCommittedAnnotation(annotation) {
    const kind = annotation.kind || {};
    const bounds = physicalToCssRect(annotation.bounds);
    const color = strokeColor(annotation);
    const width = strokeWidth(annotation);
    const opacity = strokeOpacity(annotation);
    const style = renderStyle();
    if (kind.type === "rectangle") {
      addCommitted(new Konva.Rect({ x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height, stroke: color, strokeWidth: width, opacity }));
    } else if (kind.type === "oval") {
      addCommitted(new Konva.Ellipse({ x: bounds.x + bounds.width / 2, y: bounds.y + bounds.height / 2, radiusX: bounds.width / 2, radiusY: bounds.height / 2, stroke: color, strokeWidth: width, opacity }));
    } else if (kind.type === "line") {
      const start = physicalToCssPoint(kind.start);
      const end = physicalToCssPoint(kind.end);
      addCommitted(new Konva.Line({ points: [start.x, start.y, end.x, end.y], stroke: color, strokeWidth: width, opacity, lineCap: "round" }));
    } else if (kind.type === "arrow") {
      const start = physicalToCssPoint(kind.start);
      const end = physicalToCssPoint(kind.end);
      const head = arrowHeadSize(width, style);
      addCommitted(new Konva.Arrow({ points: [start.x, start.y, end.x, end.y], stroke: color, fill: color, strokeWidth: width, opacity, pointerLength: head.length, pointerWidth: head.width, lineCap: "round" }));
    } else if (kind.type === "pen" || kind.type === "penArrow") {
      const points = smoothFlatPoints(flattenPhysicalPoints(kind.points));
      if (points.length < 4) {
        return;
      }
      if (kind.type === "penArrow") {
        drawPenArrowPath(committedTarget, points, color, width, opacity);
      } else {
        addCommitted(new Konva.Line({ points, stroke: color, strokeWidth: width, opacity, lineCap: "round", lineJoin: "round", tension: style.penTension == null ? 0.35 : style.penTension }));
      }
    } else if (kind.type === "highlighter") {
      drawCommittedHighlighter(annotation, kind, bounds, color, width);
    } else if (kind.type === "mosaic") {
      return;
    } else if (kind.type === "text") {
      drawCommittedText(annotation, kind, bounds, color);
    } else if (kind.type === "watermark") {
      drawCommittedWatermark(annotation, kind, bounds, color);
    } else if (kind.type === "tag") {
      drawCommittedTag(annotation, kind, bounds, color);
    } else if (kind.type === "step") {
      drawStepBadge(explicitStepBadgeCenter(annotation), kind.number, annotation.id, strokeColor(annotation));
    }
  }

  function arrowHeadSize(width, style) {
    const min = style.arrowMinHead || 12;
    return {
      length: Math.max(min, width * (style.arrowHeadLengthFactor || 3)),
      width: Math.max(min, width * (style.arrowHeadWidthFactor || 3)),
    };
  }

  function stableArrowTail(flatPoints, headSize) {
    const n = flatPoints.length;
    if (n < 4) {
      return null;
    }
    const end = { x: flatPoints[n - 2], y: flatPoints[n - 1] };
    for (let i = n - 4; i >= 0; i -= 2) {
      const point = { x: flatPoints[i], y: flatPoints[i + 1] };
      if (Math.hypot(end.x - point.x, end.y - point.y) >= Math.max(8, headSize * 0.7)) {
        return point;
      }
    }
    return { x: flatPoints[0], y: flatPoints[1] };
  }

  function drawArrowTip(layer, flatPoints, color, width, opacity) {
    const headSize = Math.max(12, width * 3);
    const n = flatPoints.length;
    const tip = n >= 2 ? { x: flatPoints[n - 2], y: flatPoints[n - 1] } : null;
    const tail = stableArrowTail(flatPoints, headSize);
    if (!tip || !tail) {
      return;
    }
    const angle = Math.atan2(tip.y - tail.y, tip.x - tail.x);
    const back = {
      x: tip.x - Math.cos(angle) * headSize,
      y: tip.y - Math.sin(angle) * headSize,
    };
    const spread = headSize * 0.48;
    const perp = angle + Math.PI / 2;
    layer.add(
      new Konva.Line({
        points: [
          tip.x,
          tip.y,
          back.x + Math.cos(perp) * spread,
          back.y + Math.sin(perp) * spread,
          back.x - Math.cos(perp) * spread,
          back.y - Math.sin(perp) * spread,
        ],
        closed: true,
        fill: color,
        strokeWidth: 0,
        opacity,
      })
    );
  }

  function drawPenArrowPath(layer, flatPoints, color, width, opacity) {
    if (flatPoints.length < 4) {
      return;
    }
    const headSize = Math.max(12, width * 3);
    const shaftPoints = trimmedArrowShaft(flatPoints, headSize * 0.72);
    layer.add(
      new Konva.Line({
        points: shaftPoints,
        stroke: color,
        strokeWidth: width,
        opacity,
        lineCap: "round",
        lineJoin: "round",
        tension: 0.35,
      })
    );
    drawArrowTip(layer, flatPoints, color, width, opacity);
  }

  function trimmedArrowShaft(flatPoints, trim) {
    if (flatPoints.length < 4) {
      return flatPoints;
    }
    const out = flatPoints.slice();
    const n = out.length;
    const tip = { x: out[n - 2], y: out[n - 1] };
    for (let i = n - 4; i >= 0; i -= 2) {
      const point = { x: out[i], y: out[i + 1] };
      const distance = Math.hypot(tip.x - point.x, tip.y - point.y);
      if (distance > trim) {
        const t = (distance - trim) / distance;
        out.length = i + 2;
        out.push(point.x + (tip.x - point.x) * t, point.y + (tip.y - point.y) * t);
        return out;
      }
    }
    return flatPoints.slice(0, Math.max(4, flatPoints.length - 2));
  }

  function smoothFlatPoints(flatPoints) {
    if (!flatPoints || flatPoints.length < 8) {
      return flatPoints || [];
    }
    let points = flatPoints.slice();
    for (let pass = 0; pass < 2; pass++) {
      const next = [points[0], points[1]];
      for (let i = 2; i < points.length - 2; i += 2) {
        const prevX = points[i - 2];
        const prevY = points[i - 1];
        const x = points[i];
        const y = points[i + 1];
        const nextX = points[i + 2];
        const nextY = points[i + 3];
        next.push(prevX * 0.22 + x * 0.56 + nextX * 0.22, prevY * 0.22 + y * 0.56 + nextY * 0.22);
      }
      next.push(points[points.length - 2], points[points.length - 1]);
      points = next;
    }
    return points;
  }

  function drawCommittedHighlighter(annotation, kind, bounds, color, width) {
    const style = renderStyle();
    const alpha = kind.opacity == null ? style.highlighterOpacity || 0.3 : kind.opacity;
    if (kind.shape === "line") {
      const start = physicalToCssPoint(kind.start);
      const end = physicalToCssPoint(kind.end);
      const highlighterWidth = highlighterLineRenderWidth(width, style);
      addCommitted(new Konva.Line({ points: [start.x, start.y, end.x, end.y], stroke: color, opacity: alpha, strokeWidth: highlighterWidth, lineCap: "round" }));
    } else {
      addCommitted(new Konva.Rect({ x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height, cornerRadius: physicalScalar(style.cornerRadius || 8), fill: color, opacity: alpha }));
    }
  }

  function drawMosaic(layer, bounds) {
    const style = renderStyle();
    const radius = physicalScalar(8);
    const cell = Math.max(6, physicalScalar(style.mosaicCell || 12));
    const canvas = mosaicCanvasFor(bounds, cell, radius);
    layer.add(new Konva.Image({ x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height, image: canvas }));
  }

  function mosaicCanvasFor(bounds, cell, radius) {
    const width = Math.max(1, Math.ceil(bounds.width));
    const height = Math.max(1, Math.ceil(bounds.height));
    const normalizedCell = Math.max(1, Math.round(cell));
    const normalizedRadius = Math.max(0, Math.round(radius));
    const key = `${width}:${height}:${normalizedCell}:${normalizedRadius}`;
    const cached = mosaicCanvasCache.get(key);
    if (cached) {
      return cached;
    }
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext("2d", { alpha: true });
    if (!ctx) {
      return canvas;
    }
    roundedRectPath(ctx, 0, 0, width, height, normalizedRadius);
    ctx.clip();
    for (let y = 0; y < height; y += normalizedCell) {
      for (let x = 0; x < width; x += normalizedCell) {
        ctx.fillStyle = mosaicShade(x, y);
        ctx.fillRect(x, y, Math.min(normalizedCell, width - x), Math.min(normalizedCell, height - y));
      }
    }
    if (width * height <= 4_000_000) {
      mosaicCanvasCache.set(key, canvas);
      while (mosaicCanvasCache.size > 8) {
        mosaicCanvasCache.delete(mosaicCanvasCache.keys().next().value);
      }
    }
    return canvas;
  }

  function roundedRectPath(ctx, x, y, width, height, radius) {
    const r = Math.max(0, Math.min(radius, width / 2, height / 2));
    ctx.beginPath();
    ctx.moveTo(x + r, y);
    ctx.lineTo(x + width - r, y);
    ctx.quadraticCurveTo(x + width, y, x + width, y + r);
    ctx.lineTo(x + width, y + height - r);
    ctx.quadraticCurveTo(x + width, y + height, x + width - r, y + height);
    ctx.lineTo(x + r, y + height);
    ctx.quadraticCurveTo(x, y + height, x, y + height - r);
    ctx.lineTo(x, y + r);
    ctx.quadraticCurveTo(x, y, x + r, y);
    ctx.closePath();
  }

  function mosaicShade(x, y) {
    const seed = Math.sin(Math.floor(x / 7) * 12.9898 + Math.floor(y / 7) * 78.233) * 43758.5453;
    const t = seed - Math.floor(seed);
    if (t < 0.33) return "#2E2E2E";
    if (t < 0.66) return "#8A8A8A";
    return "#D8D8D8";
  }

  function drawCommittedText(annotation, kind, bounds, color) {
    const fontSize = physicalScalar(kind.fontSize || state.fontSize || 27);
    const padding = physicalScalar(8);
    const text = kind.text || "";
    const filled = !!kind.filled;
    const framed = !!kind.framed;
    const editingWithLexical = textEditorSession && textEditorSession.id === annotation.id;
    if (framed) {
      if (filled) {
        addCommitted(new Konva.Rect({ x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height, cornerRadius: physicalScalar(12), fill: color }));
      }
      if (!editingWithLexical) {
        const textNode = new Konva.Text({
          x: bounds.x + padding,
          y: bounds.y + padding,
          width: Math.max(1, bounds.width - padding * 2),
          text,
          fontFamily: "Segoe UI",
          fontSize,
          lineHeight: 1.22,
          fill: filled ? contrastTextColor(color) : color,
        });
        addCommitted(textNode);
      }
      if (state.editingTextId === annotation.id && !editingWithLexical) {
        addCommitted(new Konva.Rect({ x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height, dash: [5, 4], stroke: selectionColor(), strokeWidth: Math.max(1, physicalScalar(1)) }));
        const caret = textCaretForIndex(text, state.editingTextCaret || 0, bounds.x + padding, bounds.y + padding, fontSize, fontSize * 1.22, bounds.width - padding * 2);
        drawTextCaret(caret.x, caret.y, caret.height);
      }
    } else {
      const textY = bounds.y - fontSize * 1.08;
      const lineHeight = fontSize * 1.22;
      const inlineLines = String(text || "").split(/\r?\n/);
      if (filled && text) {
        inlineLines.forEach((line, index) => {
          const lineWidth = Math.max(fontSize * 0.45, measureTextWidth(line, fontSize, false));
          addCommitted(new Konva.Rect({
            x: bounds.x - padding,
            y: textY + index * lineHeight - padding * 0.45,
            width: lineWidth + padding * 2,
            height: lineHeight + padding * 0.45,
            cornerRadius: physicalScalar(7),
            fill: color,
          }));
        });
      }
      if (!editingWithLexical) {
        const textNode = new Konva.Text({
          x: bounds.x,
          y: textY,
          text,
          fontFamily: "Segoe UI",
          fontSize,
          lineHeight: 1.22,
          fill: filled ? contrastTextColor(color) : color,
        });
        addCommitted(textNode);
      }
      if (state.editingTextId === annotation.id && !editingWithLexical) {
        const caret = textCaretForIndex(text, state.editingTextCaret || 0, bounds.x, textY, fontSize, lineHeight, Infinity);
        drawTextCaret(caret.x, caret.y, fontSize * 1.25);
      }
    }
  }

  function textCaretForIndex(text, caretIndex, originX, originY, fontSize, lineHeight, maxWidth) {
    const layout = textVisualLines(text, fontSize, maxWidth);
    const safeIndex = clampCaretIndex(text, caretIndex);
    const lineIndex = visualLineIndexForCaret(layout, safeIndex);
    const line = layout[lineIndex] || { text: "", start: 0, end: 0 };
    const chars = Array.from(String(text || ""));
    const lineChars = chars.slice(line.start, Math.min(safeIndex, line.end)).join("");
    const textWidth = measureTextWidth(lineChars, fontSize, false);
    return {
      x: Math.min(originX + Math.max(1, maxWidth), originX + textWidth + physicalScalar(2)),
      y: originY + lineIndex * lineHeight,
      height: lineHeight,
    };
  }

  function textVisualLines(text, fontSize, maxWidth) {
    const chars = Array.from(String(text || ""));
    const widthLimit = Number.isFinite(maxWidth) ? Math.max(fontSize, maxWidth) : Infinity;
    const lines = [];
    let hardStart = 0;
    const pushWrapped = (start, end) => {
      if (start >= end) {
        lines.push({ text: "", start, end });
        return;
      }
      let lineStart = start;
      let index = start;
      while (index < end) {
        let lastFit = index + 1;
        let lastBreak = -1;
        let probe = index + 1;
        while (probe <= end) {
          const slice = chars.slice(lineStart, probe).join("");
          if (measureTextWidth(slice, fontSize, false) > widthLimit && probe > lineStart + 1) {
            break;
          }
          lastFit = probe;
          if (/\s/.test(chars[probe - 1] || "")) {
            lastBreak = probe;
          }
          probe += 1;
        }
        let lineEnd = lastFit;
        if (lineEnd < end && lastBreak > lineStart) {
          lineEnd = lastBreak;
        }
        const visibleEnd = lineEnd;
        while (lineEnd < end && /\s/.test(chars[lineEnd] || "")) {
          lineEnd += 1;
        }
        lines.push({ text: chars.slice(lineStart, visibleEnd).join("").trimEnd(), start: lineStart, end: visibleEnd });
        lineStart = lineEnd;
        index = lineEnd;
      }
    };
    for (let index = 0; index <= chars.length; index += 1) {
      if (index === chars.length || chars[index] === "\n") {
        pushWrapped(hardStart, index);
        hardStart = index + 1;
      }
    }
    return lines.length > 0 ? lines : [{ text: "", start: 0, end: 0 }];
  }

  function visualLineIndexForCaret(layout, caretIndex) {
    for (let index = 0; index < layout.length; index += 1) {
      const line = layout[index];
      const isLast = index === layout.length - 1;
      if (caretIndex >= line.start && (caretIndex <= line.end || isLast)) {
        return index;
      }
    }
    return Math.max(0, layout.length - 1);
  }

  function clampCaretIndex(text, caretIndex) {
    const length = Array.from(String(text || "")).length;
    return Math.max(0, Math.min(length, caretIndex || 0));
  }

  function textCaretIndexFromPoint(text, point, originX, originY, fontSize, lineHeight, maxWidth) {
    const layout = textVisualLines(text, fontSize, maxWidth);
    const lineIndex = Math.max(0, Math.min(layout.length - 1, Math.floor((point.y - originY) / Math.max(1, lineHeight))));
    const line = layout[lineIndex] || { start: 0, end: 0 };
    const chars = Array.from(String(text || ""));
    const localX = Math.max(0, point.x - originX);
    let bestIndex = line.start;
    let bestDistance = Infinity;
    for (let index = line.start; index <= line.end; index += 1) {
      const slice = chars.slice(line.start, index).join("");
      const x = measureTextWidth(slice, fontSize, false);
      const distance = Math.abs(x - localX);
      if (distance < bestDistance) {
        bestDistance = distance;
        bestIndex = index;
      }
    }
    return clampCaretIndex(text, bestIndex);
  }

  function editingTextMetrics(annotation) {
    if (!annotation || !annotation.kind) {
      return null;
    }
    const kind = annotation.kind;
    const bounds = physicalToCssRect(annotation.bounds);
    const fontSize = physicalScalar(kind.fontSize || state.fontSize || 27);
    const padding = physicalScalar(8);
    if (kind.type === "text") {
      if (kind.framed) {
        return {
          text: kind.text || "",
          originX: bounds.x + padding,
          originY: bounds.y + padding,
          fontSize,
          lineHeight: fontSize * 1.22,
          maxWidth: Math.max(1, bounds.width - padding * 2),
        };
      }
      return {
        text: kind.text || "",
        originX: bounds.x,
        originY: bounds.y - fontSize * 1.08,
        fontSize,
        lineHeight: fontSize * 1.22,
        maxWidth: Infinity,
      };
    }
    if (kind.type === "tag") {
      return {
        text: kind.label || "",
        originX: bounds.x + padding,
        originY: bounds.y + padding,
        fontSize,
        lineHeight: fontSize * 1.22,
        maxWidth: Math.max(1, bounds.width - padding * 2),
      };
    }
    return null;
  }

  function textAnnotationAtPoint(point) {
    if (!state || !Array.isArray(state.annotations)) {
      return null;
    }
    for (let i = state.annotations.length - 1; i >= 0; i -= 1) {
      const annotation = state.annotations[i];
      const kind = annotation.kind || {};
      if ((kind.type === "text" || kind.type === "tag") && annotationContainsPoint(annotation, point)) {
        const bounds = physicalToCssRect(annotation.bounds);
        if (kind.type !== "text" || kind.framed || pointInRect(point, inlineTextHitBounds(bounds, kind), 0)) {
          return annotation;
        }
      }
    }
    return null;
  }

  function textCaretIndexAtPoint(point) {
    const annotation = textAnnotationAtPoint(point);
    const metrics = editingTextMetrics(annotation);
    if (!metrics) {
      return null;
    }
    return textCaretIndexFromPoint(metrics.text, point, metrics.originX, metrics.originY, metrics.fontSize, metrics.lineHeight, metrics.maxWidth);
  }

  function activeEditingTextAnnotation() {
    if (!state || state.editingTextId == null || !Array.isArray(state.annotations)) {
      return null;
    }
    return state.annotations.find((annotation) => annotation.id === state.editingTextId) || null;
  }

  function moveEditingCaretForArrow(key) {
    const annotation = activeEditingTextAnnotation();
    const metrics = editingTextMetrics(annotation);
    if (!metrics) {
      return false;
    }
    const caret = clampCaretIndex(metrics.text, state.editingTextCaret || 0);
    let next = caret;
    if (key === "ArrowLeft") {
      next = Math.max(0, caret - 1);
    } else if (key === "ArrowRight") {
      next = Math.min(Array.from(String(metrics.text || "")).length, caret + 1);
    } else {
      const layout = textVisualLines(metrics.text, metrics.fontSize, metrics.maxWidth);
      const lineIndex = visualLineIndexForCaret(layout, caret);
      const caretPoint = textCaretForIndex(metrics.text, caret, metrics.originX, metrics.originY, metrics.fontSize, metrics.lineHeight, metrics.maxWidth);
      const targetLineIndex = key === "ArrowUp" ? lineIndex - 1 : lineIndex + 1;
      if (targetLineIndex < 0 || targetLineIndex >= layout.length) {
        next = caret;
      } else {
        next = textCaretIndexFromPoint(
          metrics.text,
          { x: caretPoint.x, y: metrics.originY + targetLineIndex * metrics.lineHeight + metrics.lineHeight * 0.4 },
          metrics.originX,
          metrics.originY,
          metrics.fontSize,
          metrics.lineHeight,
          metrics.maxWidth
        );
      }
    }
    state = { ...state, editingTextCaret: next };
    showCaretNow();
    scheduleRender(state, { committed: true });
    command("setTextCaret", { caretIndex: next });
    return true;
  }

  function drawTextCaret(x, y, height) {
    if (!caretVisible && Date.now() >= caretForceVisibleUntil) {
      return;
    }
    const outer = Math.max(2, physicalScalar(3));
    const inner = Math.max(1, physicalScalar(1));
    addCommitted(new Konva.Line({ points: [x, y, x, y + height], stroke: "#000000", strokeWidth: outer, opacity: 0.95, lineCap: "round" }));
    addCommitted(new Konva.Line({ points: [x, y, x, y + height], stroke: "#FFFFFF", strokeWidth: inner, opacity: 1, lineCap: "round" }));
  }

  function measureTextWidth(text, fontSize, bold) {
    if (!text) {
      return 0;
    }
    if (!watermarkMeasureContext) {
      return text.length * fontSize * 0.56;
    }
    watermarkMeasureContext.font = `${bold ? "700 " : ""}${fontSize}px Segoe UI`;
    return watermarkMeasureContext.measureText(text).width || 0;
  }
  function invertedColorAt(_x, _y, background) {
    return invertHexColor(background || "#FFFFFF");
  }

  function invertHexColor(hex) {
    const value = String(hex || "#FFFFFF").replace("#", "");
    const r = 255 - parseInt(value.slice(0, 2) || "FF", 16);
    const g = 255 - parseInt(value.slice(2, 4) || "FF", 16);
    const b = 255 - parseInt(value.slice(4, 6) || "FF", 16);
    return `#${[r, g, b].map((n) => Math.max(0, Math.min(255, n)).toString(16).padStart(2, "0")).join("")}`.toUpperCase();
  }

  function drawCommittedWatermark(annotation, kind, bounds, color) {
    const fontSize = physicalScalar(kind.fontSize || state.fontSize || 27);
    const opacity = kind.opacity == null ? strokeOpacity(annotation) : kind.opacity;
    addCommitted(new Konva.Text({
      x: bounds.x,
      y: bounds.y,
      text: kind.text || "",
      fontFamily: "Segoe UI",
      fontSize,
      lineHeight: 1.22,
      fill: color,
      opacity,
    }));
  }

  function drawCommittedTag(annotation, kind, bounds, color) {
    const style = renderStyle();
    const baseFrame = tagBaseFrame(style);
    const frame = tagFrameForAnnotation(annotation, style);
    const radius = physicalScalar(style.tagRadius || 10);
    const padding = physicalScalar(style.tagInnerPad || 8);
    const anchor = physicalToCssPoint(kind.anchor);
    const fontSize = physicalScalar(kind.fontSize || state.fontSize || 27);
    drawTagBody(committedTarget, bounds, anchor, color, frame, radius, baseFrame);
    const inner = bounds;
    addCommitted(new Konva.Rect({ x: inner.x, y: inner.y, width: inner.width, height: inner.height, cornerRadius: Math.max(1, radius), fill: "#FFFFFF" }));
    const editingWithLexical = textEditorSession && textEditorSession.id === annotation.id;
    if (!editingWithLexical) {
      addCommitted(new Konva.Text({ x: inner.x + padding, y: inner.y + padding, width: Math.max(1, inner.width - padding * 2), text: kind.label || "", fontFamily: "Segoe UI", fontSize, lineHeight: 1.22, fill: "#000000" }));
    }
    if (state.editingTextId === annotation.id && !editingWithLexical) {
      const caret = textCaretForIndex(kind.label || "", state.editingTextCaret || 0, inner.x + padding, inner.y + padding, fontSize, fontSize * 1.22, inner.width - padding * 2);
      drawTextCaret(caret.x, caret.y, caret.height);
    }
  }

  function tagBaseFrame(style) {
    return physicalScalar(style.tagFrame || 14);
  }

  function tagFrameForAnnotation(annotation, style) {
    const base = tagBaseFrame(style);
    const width = annotation && annotation.stroke ? annotation.stroke.width || 0 : 0;
    return width >= 6 ? Math.max(6, Math.min(28, width)) : base;
  }

  function drawTagBody(layer, box, anchor, color, frame, radius, pointerFrame) {
    const fixedPointerFrame = pointerFrame == null ? frame : pointerFrame;
    const pointerBox = expandRect(box, fixedPointerFrame);
    const cornerConnector = tagCornerConnector(pointerBox, anchor, fixedPointerFrame, radius);
    if (cornerConnector) {
      layer.add(
        new Konva.Shape({
          sceneFunc: (ctx, shape) => {
            ctx.beginPath();
            ctx.moveTo(cornerConnector.base1.x, cornerConnector.base1.y);
            ctx.lineTo(cornerConnector.tip1.x, cornerConnector.tip1.y);
            ctx.quadraticCurveTo(cornerConnector.anchor.x, cornerConnector.anchor.y, cornerConnector.tip2.x, cornerConnector.tip2.y);
            ctx.lineTo(cornerConnector.base2.x, cornerConnector.base2.y);
            ctx.closePath();
            ctx.fillStrokeShape(shape);
          },
          fill: color,
          stroke: color,
          strokeWidth: 0,
        })
      );
    } else {
      const geom = tagPointerGeometry(pointerBox, anchor, fixedPointerFrame, radius);
      layer.add(
        new Konva.Shape({
          sceneFunc: (ctx, shape) => {
            ctx.beginPath();
            ctx.moveTo(geom.base1.x, geom.base1.y);
            ctx.lineTo(geom.tip1.x, geom.tip1.y);
            ctx.quadraticCurveTo(geom.anchor.x, geom.anchor.y, geom.tip2.x, geom.tip2.y);
            ctx.lineTo(geom.base2.x, geom.base2.y);
            ctx.closePath();
            ctx.fillStrokeShape(shape);
          },
          fill: color,
          stroke: color,
          strokeWidth: 0,
        })
      );
    }
    const fillBox = expandRect(box, frame);
    layer.add(new Konva.Rect({ x: fillBox.x, y: fillBox.y, width: fillBox.width, height: fillBox.height, cornerRadius: radius + frame, fill: color }));
  }

  function expandRect(rect, amount) {
    return { x: rect.x - amount, y: rect.y - amount, width: rect.width + amount * 2, height: rect.height + amount * 2 };
  }

  function tagCornerConnector(box, anchor, frame, radius) {
    const outsideX = anchor.x < box.x || anchor.x > box.x + box.width;
    const outsideY = anchor.y < box.y || anchor.y > box.y + box.height;
    if (!outsideX || !outsideY) {
      return null;
    }
    const r = Math.max(0, Math.min(radius, box.width / 2, box.height / 2));
    const inset = Math.max(frame + Math.max(3, frame * 0.35), r * 0.65);
    const center = Point2(anchor.x < box.x ? box.x + inset : box.x + box.width - inset, anchor.y < box.y ? box.y + inset : box.y + box.height - inset);
    return pointerGeometryFromCenter(center, anchor, Math.max(6, frame * 0.77), Math.max(2.1, frame * 0.2), frame * 0.72);
  }

  function tagPointerGeometry(box, anchor, frame, radius) {
    let edge;
    if (anchor.y < box.y) {
      edge = "top";
    } else if (anchor.y > box.y + box.height) {
      edge = "bottom";
    } else if (anchor.x < box.x) {
      edge = "left";
    } else if (anchor.x > box.x + box.width) {
      edge = "right";
    } else {
      edge = "bottom";
    }
    const r = Math.max(0, Math.min(radius, box.width / 2, box.height / 2));
    const maxHorizontalHalf = Math.max(7, (box.width - r * 2) / 2 - 1);
    const maxVerticalHalf = Math.max(7, (box.height - r * 2) / 2 - 1);
    const sideHalf = Math.max(6, frame * 0.77);
    const horizontalHalf = Math.max(4, Math.min(sideHalf, maxHorizontalHalf));
    const verticalHalf = Math.max(4, Math.min(sideHalf, maxVerticalHalf));
    const overlap = Math.max(6, frame * 1.08);
    const tipRound = frame * 0.72;
    if (edge === "left") {
      const y = Math.max(box.y + r + verticalHalf, Math.min(box.y + box.height - r - verticalHalf, anchor.y));
      return Object.assign(pointerGeometryFromBase(Point2(box.x + overlap, y + verticalHalf), Point2(box.x + overlap, y - verticalHalf), anchor, tipRound), { edge, anchor });
    }
    if (edge === "right") {
      const y = Math.max(box.y + r + verticalHalf, Math.min(box.y + box.height - r - verticalHalf, anchor.y));
      return Object.assign(pointerGeometryFromBase(Point2(box.x + box.width - overlap, y - verticalHalf), Point2(box.x + box.width - overlap, y + verticalHalf), anchor, tipRound), { edge, anchor });
    }
    if (edge === "top") {
      const x = Math.max(box.x + r + horizontalHalf, Math.min(box.x + box.width - r - horizontalHalf, anchor.x));
      return Object.assign(pointerGeometryFromBase(Point2(x - horizontalHalf, box.y + overlap), Point2(x + horizontalHalf, box.y + overlap), anchor, tipRound), { edge, anchor });
    }
    const x = Math.max(box.x + r + horizontalHalf, Math.min(box.x + box.width - r - horizontalHalf, anchor.x));
    return Object.assign(pointerGeometryFromBase(Point2(x + horizontalHalf, box.y + box.height - overlap), Point2(x - horizontalHalf, box.y + box.height - overlap), anchor, tipRound), { edge, anchor });
  }

  function Point2(x, y) {
    return { x, y };
  }

  function pointerGeometryFromCenter(baseCenter, anchor, baseHalf, tipHalf, round) {
    const vx = anchor.x - baseCenter.x;
    const vy = anchor.y - baseCenter.y;
    const len = Math.max(1, Math.hypot(vx, vy));
    const ux = vx / len;
    const uy = vy / len;
    const px = -uy;
    const py = ux;
    const r = Math.min(round, len * 0.4);
    const base1 = { x: baseCenter.x + px * baseHalf, y: baseCenter.y + py * baseHalf };
    const base2 = { x: baseCenter.x - px * baseHalf, y: baseCenter.y - py * baseHalf };
    const tipA = { x: anchor.x - ux * r + px * tipHalf, y: anchor.y - uy * r + py * tipHalf };
    const tipB = { x: anchor.x - ux * r - px * tipHalf, y: anchor.y - uy * r - py * tipHalf };
    const sameOrder = distanceSquared(base1, tipA) + distanceSquared(base2, tipB);
    const swappedOrder = distanceSquared(base1, tipB) + distanceSquared(base2, tipA);
    return {
      base1,
      base2,
      tip1: sameOrder <= swappedOrder ? tipA : tipB,
      tip2: sameOrder <= swappedOrder ? tipB : tipA,
      anchor,
    };
  }

  function distanceSquared(a, b) {
    const dx = a.x - b.x;
    const dy = a.y - b.y;
    return dx * dx + dy * dy;
  }

  function pointerGeometryFromBase(base1, base2, anchor, round) {
    const mid = { x: (base1.x + base2.x) / 2, y: (base1.y + base2.y) / 2 };
    const vx = anchor.x - mid.x;
    const vy = anchor.y - mid.y;
    const len = Math.max(1, Math.hypot(vx, vy));
    const ux = vx / len;
    const uy = vy / len;
    const px = -uy;
    const py = ux;
    const r = Math.min(round, len * 0.4);
    const tipA = { x: anchor.x - ux * r + px * r * 0.45, y: anchor.y - uy * r + py * r * 0.45 };
    const tipB = { x: anchor.x - ux * r - px * r * 0.45, y: anchor.y - uy * r - py * r * 0.45 };
    const sameOrder = distanceSquared(base1, tipA) + distanceSquared(base2, tipB);
    const swappedOrder = distanceSquared(base1, tipB) + distanceSquared(base2, tipA);
    return {
      base1,
      base2,
      tip1: sameOrder <= swappedOrder ? tipA : tipB,
      tip2: sameOrder <= swappedOrder ? tipB : tipA,
    };
  }

  function drawSelection(annotation) {
    const kind = annotation.kind || {};
    const bounds = physicalToCssRect(annotation.bounds);
    const color = selectionColor();
    if (kind.type === "line" || kind.type === "arrow" || (kind.type === "highlighter" && kind.shape === "line")) {
      drawHandle(physicalToCssPoint(kind.start), color);
      drawHandle(physicalToCssPoint(kind.end), color);
    } else if (kind.type === "pen" || kind.type === "penArrow") {
      const points = smoothFlatPoints(flattenPhysicalPoints(kind.points || []));
      if (points.length >= 4) {
        drawHandle({ x: points[0], y: points[1] }, color);
        drawHandle({ x: points[points.length - 2], y: points[points.length - 1] }, color);
      }
    } else {
      const selectionBounds = kind.type === "text" && !kind.framed ? inlineTextHitBounds(bounds, kind) : bounds;
      if (kind.type === "text") {
        selectionLayer.add(new Konva.Rect({ x: selectionBounds.x, y: selectionBounds.y, width: selectionBounds.width, height: selectionBounds.height, dash: [5, 4], stroke: color, strokeWidth: Math.max(1, physicalScalar(1)) }));
      } else {
        selectionLayer.add(new Konva.Rect({ x: selectionBounds.x, y: selectionBounds.y, width: selectionBounds.width, height: selectionBounds.height, stroke: color, strokeWidth: Math.max(1, physicalScalar(1)) }));
      }
      drawHandle({ x: selectionBounds.x, y: selectionBounds.y }, color);
      drawHandle({ x: selectionBounds.x + selectionBounds.width, y: selectionBounds.y }, color);
      drawHandle({ x: selectionBounds.x + selectionBounds.width, y: selectionBounds.y + selectionBounds.height }, color);
      drawHandle({ x: selectionBounds.x, y: selectionBounds.y + selectionBounds.height }, color);
      if (kind.type === "tag") {
        drawHandle(physicalToCssPoint(kind.anchor), color);
      }
    }
  }

  function drawHandle(point, color) {
    const size = Math.max(6, physicalScalar((renderStyle().selectionHandleSize || 8)));
    selectionLayer.add(new Konva.Rect({ x: point.x - size / 2, y: point.y - size / 2, width: size, height: size, fill: color }));
  }

  function selectionColor() {
    return "#0A84FF";
  }

  function drawStepBadge(center, number, id, backgroundColor) {
    if (number == null) {
      return;
    }
    const size = 22 * scale();
    const fill = backgroundColor || activeColor();
    addCommitted(new Konva.Circle({ x: center.x, y: center.y, radius: size / 2, fill }));
    if (state.editingStepNumberId === id) {
      addCommitted(new Konva.Circle({ x: center.x, y: center.y, radius: size / 2 + Math.max(1, 2 * scale()), stroke: "#FFFFFF", strokeWidth: Math.max(1, 1.4 * scale()), shadowColor: fill, shadowBlur: 6 * scale(), shadowOpacity: 0.5 }));
    }
    addCommitted(
      new Konva.Text({
        x: center.x - size / 2,
        y: center.y - size * 0.36,
        width: size,
        align: "center",
        text: String(number),
        fontFamily: "Segoe UI",
        fontStyle: "700",
        fontSize: number < 10 ? 13 * scale() : 11 * scale(),
        fill: contrastTextColor(fill),
      })
    );
  }

  function autoStepBadgeCenter(annotation) {
    const kind = annotation.kind || {};
    const size = 22 * scale();
    if (kind.type === "line" || kind.type === "arrow" || (kind.type === "highlighter" && kind.shape === "line")) {
      const start = physicalToCssPoint(kind.start);
      const end = physicalToCssPoint(kind.end);
      return {
        x: (start.x + end.x) / 2,
        y: (start.y + end.y) / 2 - strokeWidth(annotation) / 2 - size / 2 - physicalScalar(6),
      };
    }
    if (kind.type === "pen" || kind.type === "penArrow") {
      const midpoint = polylineMidpoint((kind.points || []).map(physicalToCssPoint));
      if (midpoint) {
        return {
          x: midpoint.x,
          y: midpoint.y - strokeWidth(annotation) / 2 - size / 2 - physicalScalar(3),
        };
      }
    }
    const bounds = physicalToCssRect(annotation.bounds);
    return {
      x: bounds.x + bounds.width / 2,
      y: bounds.y - size / 2 - physicalScalar(6),
    };
  }

  function explicitStepBadgeCenter(annotation) {
    const bounds = physicalToCssRect(annotation.bounds);
    return { x: bounds.x + bounds.width / 2, y: bounds.y - 14 * scale() };
  }

  function stepBadgeHitTest(point) {
    if (!point || !state || !Array.isArray(state.annotations)) {
      return null;
    }
    const radius = 14 * scale();
    for (let i = state.annotations.length - 1; i >= 0; i--) {
      const annotation = state.annotations[i];
      const kind = annotation.kind || {};
      let number = annotation.stepNumber;
      let center = null;
      if (number != null) {
        center = autoStepBadgeCenter(annotation);
      } else if (kind.type === "step" && kind.number != null) {
        number = kind.number;
        center = explicitStepBadgeCenter(annotation);
      }
      if (number != null && center && Math.hypot(point.x - center.x, point.y - center.y) <= radius) {
        return annotation.id;
      }
    }
    return null;
  }

  function polylineMidpoint(points) {
    if (!points || points.length === 0) {
      return null;
    }
    if (points.length === 1) {
      return points[0];
    }
    let total = 0;
    for (let i = 1; i < points.length; i++) {
      total += Math.hypot(points[i].x - points[i - 1].x, points[i].y - points[i - 1].y);
    }
    if (total <= 0) {
      return points[Math.floor(points.length / 2)];
    }
    let walked = 0;
    const target = total / 2;
    for (let i = 1; i < points.length; i++) {
      const start = points[i - 1];
      const end = points[i];
      const length = Math.hypot(end.x - start.x, end.y - start.y);
      if (walked + length >= target) {
        const t = (target - walked) / Math.max(1, length);
        return {
          x: start.x + (end.x - start.x) * t,
          y: start.y + (end.y - start.y) * t,
        };
      }
      walked += length;
    }
    return points[points.length - 1];
  }

  function ensurePreviewDynamicGroup() {
    if (!previewDynamicGroup || !previewDynamicGroup.getLayer()) {
      previewDynamicGroup = new Konva.Group({ listening: false, name: "preview-dynamic" });
      previewLayer.add(previewDynamicGroup);
    }
    return previewDynamicGroup;
  }

  function getPreviewNode(key, factory) {
    let node = previewNodes.get(key);
    if (!node || !node.getLayer()) {
      node = factory();
      node.listening(false);
      node.visible(false);
      previewLayer.add(node);
      previewNodes.set(key, node);
    }
    node.visible(true);
    return node;
  }

  function preparePreviewLayer(activeKey) {
    for (const [key, node] of previewNodes) {
      node.visible(key === activeKey);
    }
    ensurePreviewDynamicGroup().destroyChildren();
  }

  function hidePreviewLayer() {
    for (const node of previewNodes.values()) {
      node.visible(false);
    }
    ensurePreviewDynamicGroup().destroyChildren();
  }

  function renderPreview() {
    if (!preview || !state || !state.captureRegion) {
      hidePreviewLayer();
      return;
    }
    const tool = state.activeTool;
    const color = activeColor();
    const width = physicalStrokeWidth(currentWidth());
    const start = preview.start;
    const end = preview.current;
    const rect = rectFromPoints(start, end);
    if (tool === "rectangle") {
      preparePreviewLayer("rectangle");
      getPreviewNode("rectangle", () => new Konva.Rect()).setAttrs({ x: rect.x, y: rect.y, width: rect.width, height: rect.height, stroke: color, strokeWidth: width });
    } else if (tool === "oval") {
      preparePreviewLayer("oval");
      getPreviewNode("oval", () => new Konva.Ellipse()).setAttrs({ x: rect.x + rect.width / 2, y: rect.y + rect.height / 2, radiusX: rect.width / 2, radiusY: rect.height / 2, stroke: color, strokeWidth: width });
    } else if (tool === "line") {
      preparePreviewLayer("line");
      getPreviewNode("line", () => new Konva.Line({ lineCap: "round" })).setAttrs({ points: [start.x, start.y, end.x, end.y], stroke: color, strokeWidth: width });
    } else if (tool === "arrow") {
      preparePreviewLayer("arrow");
      getPreviewNode("arrow", () => new Konva.Arrow({ lineCap: "round" })).setAttrs({ points: [start.x, start.y, end.x, end.y], stroke: color, fill: color, strokeWidth: width, pointerLength: Math.max(12, width * 3), pointerWidth: Math.max(12, width * 3) });
    } else if (tool === "pen") {
      preparePreviewLayer("dynamic");
      const group = ensurePreviewDynamicGroup();
      const pts = flattenPoints(preview.points);
      if (state.penMode === "arrow") {
        drawPenArrowPath(group, smoothFlatPoints(pts), color, width, 1);
      } else {
        group.add(new Konva.Line({ points: smoothFlatPoints(pts), stroke: color, strokeWidth: width, lineCap: "round", lineJoin: "round", tension: 0.35 }));
      }
    } else if (tool === "highlighter") {
      const alpha = 0.3;
      if (state.highlighterShape === "line") {
        preparePreviewLayer("highlighter-line");
        getPreviewNode("highlighter-line", () => new Konva.Line({ lineCap: "round" })).setAttrs({ points: [start.x, start.y, end.x, end.y], stroke: color, opacity: alpha, strokeWidth: highlighterLineRenderWidth(width, renderStyle()) });
      } else {
        preparePreviewLayer("highlighter-area");
        getPreviewNode("highlighter-area", () => new Konva.Rect()).setAttrs({ x: rect.x, y: rect.y, width: rect.width, height: rect.height, cornerRadius: 8 * scale(), fill: color, opacity: alpha });
      }
    } else if (tool === "mosaic") {
      hidePreviewLayer();
      return;
    } else if (tool === "text") {
      if (rect.width > 4 && rect.height > 4) {
        preparePreviewLayer("text-area");
        getPreviewNode("text-area", () => new Konva.Rect({ dash: [5, 4] })).setAttrs({ x: rect.x, y: rect.y, width: rect.width, height: rect.height, stroke: "white", strokeWidth: 1 });
      } else {
        preparePreviewLayer("text-caret");
        const caret = getPreviewNode("text-caret", () => new Konva.Line());
        caret.setAttrs({ points: [start.x, start.y - 18, start.x, start.y + 6], stroke: "#FFFFFF", shadowColor: "#000000", shadowBlur: 0, shadowOffset: { x: 1, y: 0 }, shadowOpacity: 1, strokeWidth: Math.max(1, physicalScalar(1.5)) });
      }
    } else if (tool === "tag") {
      preparePreviewLayer("dynamic");
      drawTagPreview(start, end, color);
    } else {
      hidePreviewLayer();
    }
  }

  function drawTagPreview(anchor, current, color) {
    const s = scale();
    const box = tagBoxFromDrag(anchor, current, s);
    const baseFrame = tagBaseFrame(renderStyle());
    const frame = Math.max(6 * s, currentWidth() >= 6 ? currentWidth() : baseFrame);
    const radius = 10 * s;
    const group = ensurePreviewDynamicGroup();
    drawTagBody(group, box, anchor, color, frame, radius, baseFrame);
    group.add(new Konva.Rect({ x: box.x, y: box.y, width: box.width, height: box.height, cornerRadius: Math.max(1, radius), fill: "#FFFFFF" }));
  }

  function highlighterLineRenderWidth(width, style) {
    return (style.highlighterLineBase || 24) + Math.max(1, width || 1) * (style.highlighterLineWidthFactor || 3);
  }

  function tagBoxFromDrag(anchor, current, s) {
    const width = 146 * s;
    const height = 55 * s;
    if (Math.hypot(current.x - anchor.x, current.y - anchor.y) <= 5 * s) {
      return { x: anchor.x + 28 * s, y: anchor.y - height / 2, width, height };
    }
    const dx = current.x - anchor.x;
    const dy = current.y - anchor.y;
    if (Math.abs(dx) > width * 0.22 && Math.abs(dy) > height * 0.35) {
      return {
        x: dx >= 0 ? current.x : current.x - width,
        y: dy >= 0 ? current.y : current.y - height,
        width,
        height,
      };
    }
    if (Math.abs(dx) >= Math.abs(dy)) {
      return {
        x: dx >= 0 ? current.x : current.x - width,
        y: current.y - height / 2,
        width,
        height,
      };
    }
    return {
      x: current.x - width / 2,
      y: dy >= 0 ? current.y : current.y - height,
      width,
      height,
    };
  }

  function rectFromPoints(a, b) {
    return {
      x: Math.min(a.x, b.x),
      y: Math.min(a.y, b.y),
      width: Math.abs(a.x - b.x),
      height: Math.abs(a.y - b.y),
    };
  }

  function flattenPoints(points) {
    const out = [];
    for (const p of points) {
      out.push(p.x, p.y);
    }
    return out;
  }

  document.addEventListener("keydown", (event) => {
    if (event.ctrlKey && ["+", "-", "=", "0"].includes(event.key)) {
      event.preventDefault();
      return;
    }
    if (document.activeElement === watermarkInput) {
      return;
    }
    if (textEditorSession) {
      return;
    }
    if (state && state.editingTextId != null && ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) {
      if (moveEditingCaretForArrow(event.key)) {
        event.preventDefault();
        return;
      }
    }
    if (["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) {
      const slider = keyboardSliderForActiveTool();
      const direction = event.key === "ArrowRight" || event.key === "ArrowUp" ? 1 : -1;
      if (slider && adjustSlider(slider, direction, event.shiftKey)) {
        event.preventDefault();
        return;
      }
    }
    if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
      return;
    }
    command("keyDown", { keyCode: event.keyCode || event.which || 0, shiftKey: !!event.shiftKey });
    if (["Escape", "Enter", "Backspace", "Delete"].includes(event.key) || event.ctrlKey) {
      event.preventDefault();
    }
  });

  window.addEventListener(
    "wheel",
    (event) => {
      const slider = sliderAtStagePoint(eventStagePoint(event)) || hoveredSlider;
      if (adjustSliderFromWheel(slider, event)) {
        return;
      }
      if (event.ctrlKey) {
        event.preventDefault();
      }
    },
    { passive: false }
  );

  stage.on("wheel", (evt) => {
    const point = pointerPosition() || (evt.evt ? eventStagePoint(evt.evt) : null);
    const slider = sliderAtStagePoint(point) || hoveredSlider;
    if (adjustSliderFromWheel(slider, evt.evt)) {
      evt.cancelBubble = true;
    }
  });

  document.addEventListener("keypress", (event) => {
    if (document.activeElement === watermarkInput) {
      return;
    }
    if (textEditorSession) {
      return;
    }
    if (event.key && event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
      showCaretNow();
      command("char", { charCode: event.key.charCodeAt(0) });
      event.preventDefault();
    }
  });

  function applyRenderDiff(diff) {
    const baseState = pendingState || state;
    if (!baseState || !diff) {
      return;
    }
    const currentAnnotations = Array.isArray(baseState.annotations) ? baseState.annotations : [];
    const annotationOrder = currentAnnotations.map((annotation) => annotation.id);
    const annotationsById = new Map(currentAnnotations.map((annotation) => [annotation.id, annotation]));
    const removed = Array.isArray(diff.removed) ? new Set(diff.removed) : new Set();
    const added = Array.isArray(diff.added) ? diff.added : [];
    const updated = Array.isArray(diff.updated) ? diff.updated : [];

    for (const id of removed) {
      annotationsById.delete(id);
    }
    for (const annotation of updated) {
      if (!annotation || annotation.id == null) {
        continue;
      }
      if (!annotationsById.has(annotation.id) && !annotationOrder.includes(annotation.id)) {
        annotationOrder.push(annotation.id);
      }
      annotationsById.set(annotation.id, annotation);
    }
    for (const annotation of added) {
      if (!annotation || annotation.id == null) {
        continue;
      }
      if (!annotationsById.has(annotation.id) && !annotationOrder.includes(annotation.id)) {
        annotationOrder.push(annotation.id);
      }
      annotationsById.set(annotation.id, annotation);
    }

    const nextAnnotations = annotationOrder
      .filter((id) => !removed.has(id) && annotationsById.has(id))
      .map((id) => annotationsById.get(id));
    const patch = diff.state && typeof diff.state === "object" ? diff.state : {};
    const hasPatch = Object.keys(patch).length > 0;
    const watermarkChanged = ["watermarkText", "watermarkColor", "watermarkImageUrl", "watermarkImageDataUrl", "watermarkDateEnabled", "watermarkMode", "fontSize"].some((key) => Object.prototype.hasOwnProperty.call(patch, key));
    const committedChanged = added.length > 0 || updated.length > 0 || removed.size > 0 || watermarkChanged;
    if (
      Object.prototype.hasOwnProperty.call(patch, "editingTextCaret") &&
      baseState.editingTextCaret !== patch.editingTextCaret
    ) {
      showCaretNow();
    }
    const nextState = Object.assign({}, baseState, patch, { annotations: nextAnnotations });
    if (committedChanged) {
      queueCommittedDiff(removed, added, updated);
    }
    scheduleRender(nextState, {
      ui: hasPatch,
      committed: committedChanged,
      selection: hasPatch || committedChanged,
      preview: false,
    });
  }

  function queueCommittedDiff(removed, added, updated) {
    if (!pendingCommittedDiff) {
      pendingCommittedDiff = { removed: new Set(), changed: new Set() };
    }
    for (const id of removed) {
      pendingCommittedDiff.removed.add(id);
      pendingCommittedDiff.changed.delete(id);
    }
    for (const annotation of added.concat(updated)) {
      if (annotation && annotation.id != null) {
        pendingCommittedDiff.changed.add(annotation.id);
      }
    }
  }
  window.chrome.webview.addEventListener("message", (event) => {
    if (!event.data) {
      return;
    }
    if (event.data.type === "state") {
      scheduleRender(event.data.state);
    } else if (event.data.type === "renderDiff") {
      applyRenderDiff(event.data);
    } else if (event.data.type === "exportRequest") {
      handleExportRequest(event.data);
    }
  });

  function handleExportRequest(request) {
    const requestId = request && request.requestId;
    if (!request || request.format !== "png") {
      host({ type: "exportFailed", requestId, reason: "unsupported-format" });
      return;
    }
    if (request.backgroundRequired && (!request.background || !request.background.data)) {
      host({ type: "exportFailed", requestId, reason: "background-unavailable" });
      return;
    }
    host({ type: "exportFailed", requestId, reason: "web-export-not-enabled" });
  }

  window.addEventListener("resize", () => {
    stage.width(window.innerWidth);
    stage.height(window.innerHeight);
    scheduleRender();
  });

  window.setInterval(() => {
    if (Date.now() >= caretForceVisibleUntil) {
      caretVisible = !caretVisible;
    } else {
      caretVisible = true;
    }
    if (state && state.editingTextId) {
      scheduleRender(null, { committed: true });
    }
  }, 500);

  document.addEventListener("pointermove", (event) => {
    syncEditorPointerEvents({ x: event.clientX, y: event.clientY });
  });

  document.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    applyLocalDeselect();
    command("deselect");
  });

  function applyLocalDeselect() {
    if (!state) {
      return;
    }
    cancelTextEditor();
    state = {
      ...state,
      selectedAnnotationId: null,
      editingTextId: null,
      editingTextCaret: 0,
      editingStepNumberId: null,
      editingWatermarkText: false,
    };
    watermarkInput.blur();
    scheduleRender(state, { committed: true, selection: true, preview: true, ui: true });
  }
  host({ type: "ready" });
})();
