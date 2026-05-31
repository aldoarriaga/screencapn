(function () {
  "use strict";

  const host = (message) => {
    if (window.chrome && window.chrome.webview) {
      window.chrome.webview.postMessage(message);
    }
  };

  const ICONS = window.SCREEN_CAPTN_ICONS || {};
  const container = document.getElementById("stage");
  const watermarkInput = document.getElementById("watermark-input");
  const stage = new Konva.Stage({
    container,
    width: window.innerWidth,
    height: window.innerHeight,
  });
  const committedLayer = new Konva.Layer({ listening: false });
  const previewLayer = new Konva.Layer({ listening: false });
  const uiLayer = new Konva.Layer();
  stage.add(committedLayer);
  stage.add(previewLayer);
  stage.add(uiLayer);

  const iconCache = new Map();
  let state = null;
  let preview = null;
  let toolbarDrag = null;
  let toggleTween = null;
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
  let hoveredSlider = null;
  let sliderHotzones = [];

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

  const colors = ["#0A84FF", "#FF3B30", "#FFD60A", "#00C853", "#BF5AF2"];
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

  function scale() {
    return (state ? state.uiScale || 1 : 1) * viewportScale();
  }

  function viewportScale() {
    if (!state || !state.screen || !state.screen.width || !state.screen.height) {
      return 1;
    }
    const sx = window.innerWidth / state.screen.width;
    const sy = window.innerHeight / state.screen.height;
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
      return "#0A84FF";
    }
    const part = (n) => Math.max(0, Math.min(255, n || 0)).toString(16).padStart(2, "0");
    return `#${part(color.r)}${part(color.g)}${part(color.b)}`.toUpperCase();
  }

  function activeColor() {
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
      return tool === "text" || tool === "tag";
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
    if (tool === "text" || tool === "tag") {
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
    return Math.max(1, width || 1);
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

  function render() {
    sliderHotzones = [];
    uiLayer.destroyChildren();
    committedLayer.destroyChildren();
    previewLayer.destroyChildren();
    renderCommittedAnnotations();
    renderPreview();
    const rect = toolbarRect();
    if (!rect) {
      watermarkInput.style.display = "none";
      committedLayer.batchDraw();
      uiLayer.batchDraw();
      previewLayer.batchDraw();
      return;
    }
    drawToolbar(rect);
    drawSubmenu(rect);
    committedLayer.batchDraw();
    uiLayer.batchDraw();
    previewLayer.batchDraw();
  }

  function drawToolbar(rect) {
    const s = scale();
    const p = palette();
    const radius = 10 * s;
    const group = new Konva.Group({ x: rect.x, y: rect.y, name: "ui" });
    uiLayer.add(group);

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

  function drawSubmenu(toolbar) {
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
      x = drawOptionIcon(group, x, "lineText", !textFilled, () => command("setTextFilled", { filled: false }), p, s);
      x = drawOptionIcon(group, x, "solidText", textFilled, () => command("setTextFilled", { filled: true }), p, s);
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
      x = drawOptionIcon(group, x, "lineText", state.watermarkMode === "text", () => command("focusWatermarkText"), p, s);
      x = drawOptionIcon(group, x, "image", state.watermarkMode === "image", () => command("setWatermarkMode", { mode: "image" }), p, s);
      x = drawDivider(group, x, p, s);
      positionWatermarkInput(layout.x + x, layout.y + 3 * s, 122 * s, 18 * s);
      x += 132 * s;
      x = drawDivider(group, x, p, s);
      x = drawColors(group, x, p, s);
    } else {
      watermarkInput.style.display = "none";
    }
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
      watermark: 374,
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
    item.on("click tap", onClick);
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
      hit.add(new Konva.Rect({ x: 0, y: 0, width: box, height: box, cornerRadius: 3 * s, fill: color, stroke: selected ? p.swatchBorder : null, strokeWidth: selected ? 3 : 0, name: "ui" }));
      hit.on("click tap", () => command("setColor", { color }));
      hit.on("mouseenter", () => (container.style.cursor = "pointer"));
      hit.on("mouseleave", updateDefaultCursor);
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
  }

  watermarkInput.addEventListener("focus", () => command("focusWatermarkText"));
  watermarkInput.addEventListener("input", () => command("setWatermarkText", { text: watermarkInput.value }));
  watermarkInput.addEventListener("keydown", (event) => {
    if (event.key === "Escape" || event.key === "Enter") {
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
      const annotation = state.annotations[i];
      const kind = annotation.kind || {};
      const bounds = physicalToCssRect(annotation.bounds);
      const width = strokeWidth(annotation);
      const radius = Math.max(8, width / 2 + 8);
      if (kind.type === "line" || kind.type === "arrow" || (kind.type === "highlighter" && kind.shape === "line")) {
        if (distanceToSegment(point, physicalToCssPoint(kind.start), physicalToCssPoint(kind.end)) <= radius) {
          return true;
        }
      } else if (kind.type === "pen" || kind.type === "penArrow") {
        const pts = (kind.points || []).map(physicalToCssPoint);
        for (let j = 1; j < pts.length; j++) {
          if (distanceToSegment(point, pts[j - 1], pts[j]) <= radius) {
            return true;
          }
        }
      } else if (kind.type === "tag") {
        if (pointInRect(point, bounds, 4 * scale())) {
          return true;
        }
        const anchor = physicalToCssPoint(kind.anchor);
        if (Math.hypot(point.x - anchor.x, point.y - anchor.y) <= 14 * scale()) {
          return true;
        }
      } else if (kind.type === "text" && !kind.framed) {
        if (pointInRect(point, inlineTextHitBounds(bounds, kind), 0)) {
          return true;
        }
      } else if (pointInRect(point, bounds, 4 * scale())) {
        return true;
      }
    }
    return false;
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
      height: fontSize * 1.55 * lineCount + padding * 2,
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
    captureBrowserPointer(evt);
    watermarkInput.blur();
    const point = pointerPosition();
    if (!point) {
      return;
    }
    if (stepBadgeHitTest(point) != null) {
      return;
    }
    preview = shouldStartPreview(point) ? { start: point, current: point, points: [point] } : null;
    command("pointerDown", cssToPhysicalPoint(point));
    renderPreview();
    previewLayer.batchDraw();
  });

  stage.on("mousemove touchmove", (evt) => {
    if (toolbarDrag) {
      const point = pointerPosition();
      if (point) {
        command("setToolbarOrigin", cssToPhysicalPoint({ x: point.x - toolbarDrag.dx, y: point.y - toolbarDrag.dy }));
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
      renderPreview();
      previewLayer.batchDraw();
    }
    command("pointerMove", cssToPhysicalPoint(point));
  });

  stage.on("mouseup touchend", (evt) => {
    releaseBrowserPointer(evt);
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
    command("pointerUp", cssToPhysicalPoint(point));
    preview = null;
    renderPreview();
    previewLayer.batchDraw();
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
      return;
    }
    for (const annotation of state.annotations) {
      drawCommittedAnnotation(annotation);
      if (annotation.stepNumber != null) {
        drawStepBadge(autoStepBadgeCenter(annotation), annotation.stepNumber, annotation.id);
      }
    }
    const selected = state.annotations.find((annotation) => annotation.id === state.selectedAnnotationId);
    if (selected) {
      drawSelection(selected);
    }
  }

  function drawCommittedAnnotation(annotation) {
    const kind = annotation.kind || {};
    const bounds = physicalToCssRect(annotation.bounds);
    const color = strokeColor(annotation);
    const width = strokeWidth(annotation);
    const opacity = strokeOpacity(annotation);
    const style = renderStyle();
    if (kind.type === "rectangle") {
      committedLayer.add(new Konva.Rect({ x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height, stroke: color, strokeWidth: width, opacity }));
    } else if (kind.type === "oval") {
      committedLayer.add(new Konva.Ellipse({ x: bounds.x + bounds.width / 2, y: bounds.y + bounds.height / 2, radiusX: bounds.width / 2, radiusY: bounds.height / 2, stroke: color, strokeWidth: width, opacity }));
    } else if (kind.type === "line") {
      const start = physicalToCssPoint(kind.start);
      const end = physicalToCssPoint(kind.end);
      committedLayer.add(new Konva.Line({ points: [start.x, start.y, end.x, end.y], stroke: color, strokeWidth: width, opacity, lineCap: "round" }));
    } else if (kind.type === "arrow") {
      const start = physicalToCssPoint(kind.start);
      const end = physicalToCssPoint(kind.end);
      const head = arrowHeadSize(width, style);
      committedLayer.add(new Konva.Arrow({ points: [start.x, start.y, end.x, end.y], stroke: color, fill: color, strokeWidth: width, opacity, pointerLength: head.length, pointerWidth: head.width, lineCap: "round" }));
    } else if (kind.type === "pen" || kind.type === "penArrow") {
      const points = smoothFlatPoints(flattenPhysicalPoints(kind.points));
      if (points.length < 4) {
        return;
      }
      if (kind.type === "penArrow") {
        drawPenArrowPath(committedLayer, points, color, width, opacity);
      } else {
        committedLayer.add(new Konva.Line({ points, stroke: color, strokeWidth: width, opacity, lineCap: "round", lineJoin: "round", tension: style.penTension == null ? 0.35 : style.penTension }));
      }
    } else if (kind.type === "highlighter") {
      drawCommittedHighlighter(annotation, kind, bounds, color, width);
    } else if (kind.type === "mosaic") {
      return;
    } else if (kind.type === "text") {
      drawCommittedText(annotation, kind, bounds, color);
    } else if (kind.type === "tag") {
      drawCommittedTag(annotation, kind, bounds, color);
    } else if (kind.type === "step") {
      drawStepBadge(explicitStepBadgeCenter(annotation), kind.number, annotation.id);
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
        lineCap: "butt",
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
      const highlighterWidth = (style.highlighterLineBase || 24) + width * (style.highlighterLineWidthFactor || 3);
      committedLayer.add(new Konva.Line({ points: [start.x, start.y, end.x, end.y], stroke: color, opacity: alpha, strokeWidth: highlighterWidth, lineCap: "round" }));
    } else {
      committedLayer.add(new Konva.Rect({ x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height, cornerRadius: physicalScalar(style.cornerRadius || 8), fill: color, opacity: alpha }));
    }
  }

  function drawMosaic(layer, bounds) {
    const style = renderStyle();
    const radius = physicalScalar(8);
    const cell = Math.max(6, physicalScalar(style.mosaicCell || 12));
    const group = new Konva.Group({
      clipFunc: (ctx) => {
        roundedRectPath(ctx, bounds.x, bounds.y, bounds.width, bounds.height, radius);
      },
    });
    for (let y = bounds.y; y < bounds.y + bounds.height; y += cell) {
      for (let x = bounds.x; x < bounds.x + bounds.width; x += cell) {
        const shade = mosaicShade(x, y);
        group.add(new Konva.Rect({ x, y, width: Math.min(cell, bounds.x + bounds.width - x), height: Math.min(cell, bounds.y + bounds.height - y), fill: shade, opacity: 1 }));
      }
    }
    layer.add(group);
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
    if (framed) {
      if (filled) {
        committedLayer.add(new Konva.Rect({ x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height, cornerRadius: physicalScalar(12), fill: color }));
      }
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
      committedLayer.add(textNode);
      if (state.editingTextId === annotation.id) {
        committedLayer.add(new Konva.Rect({ x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height, dash: [5, 4], stroke: selectionColor(), strokeWidth: Math.max(1, physicalScalar(1)) }));
      }
    } else {
      const textY = bounds.y - fontSize * 1.08;
      const textNode = new Konva.Text({
        x: bounds.x,
        y: textY,
        text,
        fontFamily: "Segoe UI",
        fontSize,
        lineHeight: 1,
        fill: filled ? contrastTextColor(color) : color,
      });
      committedLayer.add(textNode);
      if (filled && text) {
        textNode.moveToTop();
        committedLayer.add(new Konva.Rect({ x: bounds.x - padding, y: textY - padding * 0.55, width: textNode.width() + padding * 2, height: fontSize * 1.28 + padding, cornerRadius: physicalScalar(12), fill: color }));
        textNode.moveToTop();
      }
      if (state.editingTextId === annotation.id && caretVisible) {
        committedLayer.add(new Konva.Line({ points: [bounds.x + textNode.width() + physicalScalar(2), textY, bounds.x + textNode.width() + physicalScalar(2), textY + fontSize * 1.25], stroke: filled ? contrastTextColor(color) : color, strokeWidth: Math.max(1, physicalScalar(1.5)) }));
      }
    }
  }

  function drawCommittedTag(annotation, kind, bounds, color) {
    const style = renderStyle();
    const frame = tagFrameForAnnotation(annotation, style);
    const radius = physicalScalar(style.tagRadius || 10);
    const padding = physicalScalar(style.tagInnerPad || 8);
    const anchor = physicalToCssPoint(kind.anchor);
    const fontSize = physicalScalar(kind.fontSize || state.fontSize || 27);
    drawTagBody(committedLayer, bounds, anchor, color, frame, radius);
    const inner = {
      x: bounds.x + frame,
      y: bounds.y + frame,
      width: Math.max(1, bounds.width - frame * 2),
      height: Math.max(1, bounds.height - frame * 2),
    };
    committedLayer.add(new Konva.Rect({ x: inner.x, y: inner.y, width: inner.width, height: inner.height, cornerRadius: Math.max(1, radius - frame / 2), fill: "#FFFFFF" }));
    committedLayer.add(new Konva.Text({ x: inner.x + padding, y: inner.y + padding, width: Math.max(1, inner.width - padding * 2), text: kind.label || "", fontFamily: "Segoe UI", fontSize, lineHeight: 1.22, fill: "#000000" }));
  }

  function tagFrameForAnnotation(annotation, style) {
    const base = physicalScalar(style.tagFrame || 14);
    const width = annotation && annotation.stroke ? annotation.stroke.width || 0 : 0;
    return width >= 6 ? Math.max(6, Math.min(28, width)) : base;
  }

  function drawTagBody(layer, box, anchor, color, frame, radius) {
    const geom = tagPointerGeometry(box, anchor, frame);
    layer.add(
      new Konva.Shape({
        sceneFunc: (ctx, shape) => {
          ctx.beginPath();
          ctx.moveTo(geom.base1.x, geom.base1.y);
          ctx.lineTo(geom.tip1.x, geom.tip1.y);
          ctx.quadraticCurveTo(anchor.x, anchor.y, geom.tip2.x, geom.tip2.y);
          ctx.lineTo(geom.base2.x, geom.base2.y);
          ctx.closePath();
          ctx.fillStrokeShape(shape);
        },
        fill: color,
        stroke: color,
        strokeWidth: 0,
      })
    );
    layer.add(new Konva.Rect({ x: box.x, y: box.y, width: box.width, height: box.height, cornerRadius: radius, fill: color }));
  }

  function tagPointerGeometry(box, anchor, frame) {
    const center = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
    const dx = anchor.x - center.x;
    const dy = anchor.y - center.y;
    let edge;
    if (Math.abs(dx / Math.max(1, box.width)) > Math.abs(dy / Math.max(1, box.height))) {
      edge = dx < 0 ? "left" : "right";
    } else {
      edge = dy < 0 ? "top" : "bottom";
    }
    const halfBase = frame * 1.15;
    const inset = frame * 1.9;
    const tipRound = frame * 0.72;
    if (edge === "left") {
      const y = Math.max(box.y + inset, Math.min(box.y + box.height - inset, anchor.y));
      return pointerGeometryFromBase(Point2(box.x, y - halfBase), Point2(box.x, y + halfBase), anchor, tipRound);
    }
    if (edge === "right") {
      const y = Math.max(box.y + inset, Math.min(box.y + box.height - inset, anchor.y));
      return pointerGeometryFromBase(Point2(box.x + box.width, y + halfBase), Point2(box.x + box.width, y - halfBase), anchor, tipRound);
    }
    if (edge === "top") {
      const x = Math.max(box.x + inset, Math.min(box.x + box.width - inset, anchor.x));
      return pointerGeometryFromBase(Point2(x + halfBase, box.y), Point2(x - halfBase, box.y), anchor, tipRound);
    }
    const x = Math.max(box.x + inset, Math.min(box.x + box.width - inset, anchor.x));
    return pointerGeometryFromBase(Point2(x - halfBase, box.y + box.height), Point2(x + halfBase, box.y + box.height), anchor, tipRound);
  }

  function Point2(x, y) {
    return { x, y };
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
    return {
      base1,
      base2,
      tip1: { x: anchor.x - ux * r + px * r * 0.45, y: anchor.y - uy * r + py * r * 0.45 },
      tip2: { x: anchor.x - ux * r - px * r * 0.45, y: anchor.y - uy * r - py * r * 0.45 },
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
      return;
    } else {
      const selectionBounds = kind.type === "text" && !kind.framed ? inlineTextHitBounds(bounds, kind) : bounds;
      if (kind.type === "text") {
        committedLayer.add(new Konva.Rect({ x: selectionBounds.x, y: selectionBounds.y, width: selectionBounds.width, height: selectionBounds.height, dash: [5, 4], stroke: color, strokeWidth: Math.max(1, physicalScalar(1)) }));
      } else {
        committedLayer.add(new Konva.Rect({ x: selectionBounds.x, y: selectionBounds.y, width: selectionBounds.width, height: selectionBounds.height, stroke: color, strokeWidth: Math.max(1, physicalScalar(1)) }));
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
    committedLayer.add(new Konva.Rect({ x: point.x - size / 2, y: point.y - size / 2, width: size, height: size, fill: color }));
  }

  function selectionColor() {
    return "#0A84FF";
  }

  function drawStepBadge(center, number, id) {
    if (number == null) {
      return;
    }
    const size = 22 * scale();
    committedLayer.add(new Konva.Circle({ x: center.x, y: center.y, radius: size / 2, fill: "#0A84FF" }));
    if (state.editingStepNumberId === id) {
      committedLayer.add(new Konva.Circle({ x: center.x, y: center.y, radius: size / 2 + Math.max(1, 2 * scale()), stroke: "#FFFFFF", strokeWidth: Math.max(1, 1.4 * scale()), shadowColor: "#0A84FF", shadowBlur: 6 * scale(), shadowOpacity: 0.5 }));
    }
    committedLayer.add(
      new Konva.Text({
        x: center.x - size / 2,
        y: center.y - size * 0.36,
        width: size,
        align: "center",
        text: String(number),
        fontFamily: "Segoe UI",
        fontStyle: "700",
        fontSize: number < 10 ? 13 * scale() : 11 * scale(),
        fill: "#FFFFFF",
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

  function renderPreview() {
    previewLayer.destroyChildren();
    if (!preview || !state || !state.captureRegion) {
      return;
    }
    const tool = state.activeTool;
    const color = activeColor();
    const width = currentWidth();
    const start = preview.start;
    const end = preview.current;
    const rect = rectFromPoints(start, end);
    if (tool === "rectangle") {
      previewLayer.add(new Konva.Rect({ x: rect.x, y: rect.y, width: rect.width, height: rect.height, stroke: color, strokeWidth: width }));
    } else if (tool === "oval") {
      previewLayer.add(new Konva.Ellipse({ x: rect.x + rect.width / 2, y: rect.y + rect.height / 2, radiusX: rect.width / 2, radiusY: rect.height / 2, stroke: color, strokeWidth: width }));
    } else if (tool === "line") {
      previewLayer.add(new Konva.Line({ points: [start.x, start.y, end.x, end.y], stroke: color, strokeWidth: width, lineCap: "round" }));
    } else if (tool === "arrow") {
      previewLayer.add(new Konva.Arrow({ points: [start.x, start.y, end.x, end.y], stroke: color, fill: color, strokeWidth: width, pointerLength: Math.max(12, width * 3), pointerWidth: Math.max(12, width * 3), lineCap: "round" }));
    } else if (tool === "pen") {
      const pts = flattenPoints(preview.points);
      if (state.penMode === "arrow") {
        drawPenArrowPath(previewLayer, smoothFlatPoints(pts), color, width, 1);
      } else {
        previewLayer.add(new Konva.Line({ points: smoothFlatPoints(pts), stroke: color, strokeWidth: width, lineCap: "round", lineJoin: "round", tension: 0.35 }));
      }
    } else if (tool === "highlighter") {
      const alpha = 0.3;
      if (state.highlighterShape === "line") {
        previewLayer.add(new Konva.Line({ points: [start.x, start.y, end.x, end.y], stroke: color, opacity: alpha, strokeWidth: 24 + width * 3, lineCap: "round" }));
      } else {
        previewLayer.add(new Konva.Rect({ x: rect.x, y: rect.y, width: rect.width, height: rect.height, cornerRadius: 8 * scale(), fill: color, opacity: alpha }));
      }
    } else if (tool === "mosaic") {
      return;
    } else if (tool === "text") {
      if (rect.width > 4 && rect.height > 4) {
        previewLayer.add(new Konva.Rect({ x: rect.x, y: rect.y, width: rect.width, height: rect.height, dash: [5, 4], stroke: "white", strokeWidth: 1 }));
      } else {
        previewLayer.add(new Konva.Line({ points: [start.x, start.y - 18, start.x, start.y + 6], stroke: color, strokeWidth: 1 }));
      }
    } else if (tool === "tag") {
      drawTagPreview(start, end, color);
    }
  }

  function drawTagPreview(anchor, current, color) {
    const s = scale();
    const box = tagBoxFromDrag(anchor, current, s);
    const frame = Math.max(6 * s, currentWidth() >= 6 ? currentWidth() : 14 * s);
    const radius = 10 * s;
    drawTagBody(previewLayer, box, anchor, color, frame, radius);
    previewLayer.add(new Konva.Rect({ x: box.x + frame, y: box.y + frame, width: Math.max(1, box.width - frame * 2), height: Math.max(1, box.height - frame * 2), cornerRadius: Math.max(1, radius - frame / 2), fill: "#FFFFFF" }));
  }

  function tagBoxFromDrag(anchor, current, s) {
    const width = 220 * s;
    const height = 110 * s;
    if (Math.hypot(current.x - anchor.x, current.y - anchor.y) <= 5 * s) {
      return { x: anchor.x + 28 * s, y: anchor.y - height / 2, width, height };
    }
    const dx = current.x - anchor.x;
    const dy = current.y - anchor.y;
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
    command("keyDown", { keyCode: event.keyCode || event.which || 0 });
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
    if (event.key && event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
      command("char", { charCode: event.key.charCodeAt(0) });
      event.preventDefault();
    }
  });

  window.chrome.webview.addEventListener("message", (event) => {
    if (!event.data || event.data.type !== "state") {
      return;
    }
    state = event.data.state;
    render();
  });

  window.addEventListener("resize", () => {
    stage.width(window.innerWidth);
    stage.height(window.innerHeight);
    render();
  });

  window.setInterval(() => {
    caretVisible = !caretVisible;
    if (state && state.editingTextId) {
      render();
    }
  }, 500);

  document.addEventListener("contextmenu", (event) => event.preventDefault());
  host({ type: "ready" });
})();
