"use client";

import {
  ArrowUpRight,
  Circle,
  Eraser,
  HighlighterCircle,
  LineSegment,
  PaintBucket,
  PaintBrush,
  Palette,
  PencilSimple,
  Rectangle,
  SlidersHorizontal,
  Star,
  Trash,
  Triangle,
  ArrowCounterClockwise,
  ArrowClockwise,
  X,
} from "@phosphor-icons/react";
import { CSSProperties, PointerEvent as ReactPointerEvent, useCallback, useEffect, useRef, useState } from "react";
import { DrawOp, Point, Stroke } from "../lib/protocol.ts";
import styles from "./ndraw.module.css";

export const CANVAS_WIDTH = 1024;
export const CANVAS_HEIGHT = 960;
const DRAW_FRAME_INTERVAL_MS = 1000 / 30;

type Tool =
  | "pen"
  | "pencil"
  | "marker"
  | "pastel"
  | "eraser"
  | "bucket"
  | "line"
  | "arrow"
  | "rectangle"
  | "ellipse"
  | "triangle"
  | "star";

type ToolPanelSection = "brushes" | "shapes" | "colors" | "size" | null;

const PALETTE = [
  "#242622", "#ffffff", "#7656df", "#ff695c", "#f5b82e", "#f8e263",
  "#66c98f", "#26a69a", "#4b9cf4", "#3e5ed8", "#a85bd5", "#ef77a8",
  "#8a5a44", "#ff9d59", "#b9d875", "#6ed4dc", "#b8c1ff", "#a78bfa",
];

const TOOL_GROUPS: { label: string; tools: { id: Tool; label: string; icon: typeof PencilSimple }[] }[] = [
  {
    label: "Draw",
    tools: [
      { id: "pen", label: "Pen", icon: PaintBrush },
      { id: "pencil", label: "Pencil", icon: PencilSimple },
      { id: "marker", label: "Marker", icon: HighlighterCircle },
      { id: "pastel", label: "Pastel", icon: Palette },
    ],
  },
  {
    label: "Edit",
    tools: [
      { id: "eraser", label: "Eraser", icon: Eraser },
      { id: "bucket", label: "Fill", icon: PaintBucket },
    ],
  },
  {
    label: "Shapes",
    tools: [
      { id: "line", label: "Line", icon: LineSegment },
      { id: "arrow", label: "Arrow", icon: ArrowUpRight },
      { id: "rectangle", label: "Rectangle", icon: Rectangle },
      { id: "ellipse", label: "Ellipse", icon: Circle },
      { id: "triangle", label: "Triangle", icon: Triangle },
      { id: "star", label: "Star", icon: Star },
    ],
  },
];

function canvasPoint(canvas: HTMLCanvasElement, event: PointerEvent | ReactPointerEvent): Point {
  const bounds = canvas.getBoundingClientRect();
  return {
    x: Math.max(0, Math.min(CANVAS_WIDTH, ((event.clientX - bounds.left) / bounds.width) * CANVAS_WIDTH)),
    y: Math.max(0, Math.min(CANVAS_HEIGHT, ((event.clientY - bounds.top) / bounds.height) * CANVAS_HEIGHT)),
  };
}

function drawStar(context: CanvasRenderingContext2D, start: Point, end: Point): void {
  const centerX = (start.x + end.x) / 2;
  const centerY = (start.y + end.y) / 2;
  const outer = Math.max(4, Math.min(Math.abs(end.x - start.x), Math.abs(end.y - start.y)) / 2);
  const inner = outer * 0.44;
  context.moveTo(centerX, centerY - outer);
  for (let index = 1; index < 10; index += 1) {
    const radius = index % 2 === 0 ? outer : inner;
    const angle = -Math.PI / 2 + (index * Math.PI) / 5;
    context.lineTo(centerX + Math.cos(angle) * radius, centerY + Math.sin(angle) * radius);
  }
  context.closePath();
}

function drawShape(context: CanvasRenderingContext2D, tool: Tool, start: Point, end: Point): void {
  const width = end.x - start.x;
  const height = end.y - start.y;
  context.beginPath();
  switch (tool) {
    case "line":
      context.moveTo(start.x, start.y);
      context.lineTo(end.x, end.y);
      break;
    case "arrow": {
      const angle = Math.atan2(end.y - start.y, end.x - start.x);
      const head = 24;
      context.moveTo(start.x, start.y);
      context.lineTo(end.x, end.y);
      context.moveTo(end.x, end.y);
      context.lineTo(end.x - head * Math.cos(angle - Math.PI / 6), end.y - head * Math.sin(angle - Math.PI / 6));
      context.moveTo(end.x, end.y);
      context.lineTo(end.x - head * Math.cos(angle + Math.PI / 6), end.y - head * Math.sin(angle + Math.PI / 6));
      break;
    }
    case "rectangle":
      context.rect(start.x, start.y, width, height);
      break;
    case "ellipse":
      context.ellipse(start.x + width / 2, start.y + height / 2, Math.abs(width / 2), Math.abs(height / 2), 0, 0, Math.PI * 2);
      break;
    case "triangle":
      context.moveTo(start.x + width / 2, start.y);
      context.lineTo(end.x, end.y);
      context.lineTo(start.x, end.y);
      context.closePath();
      break;
    case "star":
      drawStar(context, start, end);
      break;
    default:
      break;
  }
  context.stroke();
}

function prepareBrush(context: CanvasRenderingContext2D, tool: Tool, color: string, size: number): void {
  context.globalCompositeOperation = tool === "eraser" ? "destination-out" : "source-over";
  context.globalAlpha = tool === "marker" ? 0.28 : tool === "pencil" ? 0.7 : 1;
  context.strokeStyle = color;
  context.fillStyle = color;
  context.lineCap = "round";
  context.lineJoin = "round";
  context.lineWidth = tool === "marker" ? size * 2.2 : tool === "pastel" ? size * 1.35 : tool === "eraser" ? size * 2 : size;
}

function renderStroke(context: CanvasRenderingContext2D, stroke: Stroke, fromIndex = 0): void {
  const firstIndex = Math.max(0, Math.min(fromIndex, stroke.points.length - 1));
  const first = stroke.points[firstIndex];
  if (!first) return;
  context.lineCap = "round";
  context.lineJoin = "round";
  context.globalAlpha = 1;
  context.globalCompositeOperation = "source-over";
  context.strokeStyle = `#${stroke.color.toString(16).padStart(6, "0")}`;
  context.lineWidth = stroke.width;
  context.beginPath();
  context.moveTo(first.x, first.y);
  if (stroke.points.length === 1) {
    context.lineTo(first.x + 0.01, first.y + 0.01);
  } else {
    for (let index = firstIndex + 1; index < stroke.points.length - 1; index += 1) {
      const point = stroke.points[index];
      const next = stroke.points[index + 1];
      context.quadraticCurveTo(point.x, point.y, (point.x + next.x) / 2, (point.y + next.y) / 2);
    }
    const last = stroke.points[stroke.points.length - 1];
    context.lineTo(last.x, last.y);
  }
  context.stroke();
}

function renderStrokes(context: CanvasRenderingContext2D, strokes: readonly Stroke[], backgroundColor: number): void {
  context.globalAlpha = 1;
  context.globalCompositeOperation = "source-over";
  context.fillStyle = `#${backgroundColor.toString(16).padStart(6, "0")}`;
  context.fillRect(0, 0, CANVAS_WIDTH, CANVAS_HEIGHT);
  for (const stroke of strokes) renderStroke(context, stroke);
}

function pointsStartWith(points: readonly Point[], prefix: readonly Point[]): boolean {
  return prefix.length <= points.length && prefix.every((point, index) => {
    const candidate = points[index];
    return candidate?.x === point.x && candidate.y === point.y;
  });
}

function rounded(point: Point): Point {
  return { x: Math.round(point.x), y: Math.round(point.y) };
}

/**
 * Inserts logical-canvas samples when a browser reports a large pointer jump.
 * This keeps remote strokes faithful on devices that emit sparse pointer events.
 */
function sampledSegment(from: Point, to: Point, maximumStep = 8): Point[] {
  const distance = Math.hypot(to.x - from.x, to.y - from.y);
  const steps = Math.max(1, Math.ceil(distance / maximumStep));
  const points: Point[] = [];
  for (let index = 1; index <= steps; index += 1) {
    const ratio = index / steps;
    points.push({
      x: from.x + (to.x - from.x) * ratio,
      y: from.y + (to.y - from.y) * ratio,
    });
  }
  return points;
}

function shapePoints(tool: Tool, start: Point, end: Point): Point[] {
  const points: Point[] = [];
  const addLine = (from: Point, to: Point, count = 12) => {
    for (let index = 0; index <= count; index += 1) {
      const ratio = index / count;
      points.push(rounded({ x: from.x + (to.x - from.x) * ratio, y: from.y + (to.y - from.y) * ratio }));
    }
  };
  if (tool === "line") addLine(start, end, 24);
  else if (tool === "arrow") {
    addLine(start, end, 24);
    const angle = Math.atan2(end.y - start.y, end.x - start.x);
    const head = 30;
    const left = { x: end.x - head * Math.cos(angle - Math.PI / 6), y: end.y - head * Math.sin(angle - Math.PI / 6) };
    const right = { x: end.x - head * Math.cos(angle + Math.PI / 6), y: end.y - head * Math.sin(angle + Math.PI / 6) };
    addLine(end, left, 8);
    addLine(left, end, 8);
    addLine(end, right, 8);
  } else if (tool === "rectangle") {
    const topRight = { x: end.x, y: start.y };
    const bottomLeft = { x: start.x, y: end.y };
    addLine(start, topRight); addLine(topRight, end); addLine(end, bottomLeft); addLine(bottomLeft, start);
  } else if (tool === "triangle") {
    const top = { x: (start.x + end.x) / 2, y: start.y };
    const left = { x: start.x, y: end.y };
    addLine(top, end); addLine(end, left); addLine(left, top);
  } else if (tool === "ellipse") {
    const centerX = (start.x + end.x) / 2;
    const centerY = (start.y + end.y) / 2;
    const radiusX = Math.abs(end.x - start.x) / 2;
    const radiusY = Math.abs(end.y - start.y) / 2;
    for (let index = 0; index <= 48; index += 1) {
      const angle = (index / 48) * Math.PI * 2;
      points.push(rounded({ x: centerX + Math.cos(angle) * radiusX, y: centerY + Math.sin(angle) * radiusY }));
    }
  } else if (tool === "star") {
    const centerX = (start.x + end.x) / 2;
    const centerY = (start.y + end.y) / 2;
    const outer = Math.max(4, Math.min(Math.abs(end.x - start.x), Math.abs(end.y - start.y)) / 2);
    for (let index = 0; index <= 10; index += 1) {
      const radius = index % 2 === 0 ? outer : outer * 0.44;
      const angle = -Math.PI / 2 + (index * Math.PI) / 5;
      points.push(rounded({ x: centerX + Math.cos(angle) * radius, y: centerY + Math.sin(angle) * radius }));
    }
  }
  return points;
}

export function DrawingStudio({
  backgroundColor = 0xffffff,
  compact = false,
  enabled = true,
  strokes = [],
  onDraw,
}: {
  backgroundColor?: number;
  compact?: boolean;
  enabled?: boolean;
  strokes?: readonly Stroke[];
  onDraw?: (operation: DrawOp) => void;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const historyRef = useRef<ImageData[]>([]);
  const futureRef = useRef<ImageData[]>([]);
  const startRef = useRef<Point | null>(null);
  const lastRef = useRef<Point | null>(null);
  const previewRef = useRef<ImageData | null>(null);
  const activeStrokeRef = useRef<number | null>(null);
  const sequenceRef = useRef(0);
  const pendingPointsRef = useRef<Point[]>([]);
  const flushTimerRef = useRef<number | null>(null);
  const lastFlushAtRef = useRef(Number.NEGATIVE_INFINITY);
  const renderedStrokesRef = useRef<readonly Stroke[]>([]);
  const renderedBackgroundRef = useRef(0xffffff);
  const onDrawRef = useRef(onDraw);
  const [tool, setTool] = useState<Tool>("pen");
  const [color, setColor] = useState("#7656df");
  const [size, setSize] = useState(12);
  const [openSection, setOpenSection] = useState<ToolPanelSection>(null);
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);

  useEffect(() => {
    onDrawRef.current = onDraw;
  }, [onDraw]);

  const updateHistoryState = useCallback(() => {
    setCanUndo(historyRef.current.length > 0);
    setCanRedo(futureRef.current.length > 0);
  }, []);

  const saveSnapshot = useCallback(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d", { willReadFrequently: true });
    if (!canvas || !context) return;
    historyRef.current.push(context.getImageData(0, 0, CANVAS_WIDTH, CANVAS_HEIGHT));
    if (historyRef.current.length > 30) historyRef.current.shift();
    futureRef.current = [];
    updateHistoryState();
  }, [updateHistoryState]);

  useEffect(() => {
    const context = canvasRef.current?.getContext("2d", { willReadFrequently: true });
    // The server echoes the drawer's operations. Repainting those partial
    // echoes during a pointer gesture would overwrite the smoother local ink.
    if (!context || startRef.current !== null) return;

    const previous = renderedStrokesRef.current;
    const previousLast = previous[previous.length - 1];
    const currentLast = strokes[strokes.length - 1];
    const unchangedPrefixLength = Math.max(0, previous.length - 1);
    const unchangedPrefix = previous
      .slice(0, unchangedPrefixLength)
      .every((stroke, index) => stroke === strokes[index]);

    if (backgroundColor !== renderedBackgroundRef.current) {
      renderStrokes(context, strokes, backgroundColor);
    } else if (
      strokes.length === previous.length + 1
      && previous.every((stroke, index) => stroke === strokes[index])
      && currentLast
    ) {
      renderStroke(context, currentLast);
    } else if (
      strokes.length === previous.length
      && unchangedPrefix
      && previousLast
      && currentLast
      && previousLast.strokeId === currentLast.strokeId
      && previousLast.color === currentLast.color
      && previousLast.width === currentLast.width
      && pointsStartWith(currentLast.points, previousLast.points)
    ) {
      if (currentLast.points.length > previousLast.points.length) {
        renderStroke(context, currentLast, Math.max(0, previousLast.points.length - 2));
      }
    } else {
      renderStrokes(context, strokes, backgroundColor);
    }
    renderedStrokesRef.current = strokes;
    renderedBackgroundRef.current = backgroundColor;
  }, [backgroundColor, strokes]);

  const flushPoints = useCallback(() => {
    if (flushTimerRef.current !== null) {
      window.clearTimeout(flushTimerRef.current);
      flushTimerRef.current = null;
    }
    const strokeId = activeStrokeRef.current;
    if (strokeId === null || pendingPointsRef.current.length === 0 || !onDrawRef.current) return;
    while (pendingPointsRef.current.length > 0) {
      const points = pendingPointsRef.current.splice(0, 64);
      onDrawRef.current({ kind: "points", strokeId, sequence: sequenceRef.current, points });
      sequenceRef.current += 1;
    }
    lastFlushAtRef.current = performance.now();
  }, []);

  useEffect(() => () => {
    if (flushTimerRef.current !== null) window.clearTimeout(flushTimerRef.current);
  }, []);

  const sendWholeStroke = (points: Point[], strokeColor: string, strokeWidth: number) => {
    const [start, ...remaining] = points;
    if (!start || !onDrawRef.current) return;
    const random = new Uint32Array(1);
    crypto.getRandomValues(random);
    const strokeId = random[0] || 1;
    const colorValue = Number.parseInt(strokeColor.slice(1), 16);
    onDrawRef.current({ kind: "begin", strokeId, color: colorValue, width: strokeWidth, start });
    let sequence = 0;
    for (let offset = 0; offset < remaining.length; offset += 64) {
      onDrawRef.current({ kind: "points", strokeId, sequence, points: remaining.slice(offset, offset + 64) });
      sequence += 1;
    }
    onDrawRef.current({ kind: "end", strokeId, sequence });
  };

  const pointerDown = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d", { willReadFrequently: true });
    if (!canvas || !context || !enabled) return;
    canvas.setPointerCapture(event.pointerId);
    const point = canvasPoint(canvas, event);
    saveSnapshot();
    prepareBrush(context, tool, color, size);
    if (tool === "bucket") {
      context.globalCompositeOperation = "source-over";
      context.globalAlpha = 1;
      context.fillStyle = color;
      context.fillRect(0, 0, CANVAS_WIDTH, CANVAS_HEIGHT);
      for (const stroke of strokes) renderStroke(context, stroke);
      onDrawRef.current?.({ kind: "fill", color: Number.parseInt(color.slice(1), 16) });
      updateHistoryState();
      return;
    }
    startRef.current = point;
    lastRef.current = point;
    previewRef.current = context.getImageData(0, 0, CANVAS_WIDTH, CANVAS_HEIGHT);
    if (["pen", "pencil", "marker", "pastel", "eraser"].includes(tool)) {
      context.beginPath();
      context.moveTo(point.x, point.y);
      context.lineTo(point.x + 0.01, point.y + 0.01);
      context.stroke();
      if (onDrawRef.current) {
        const random = new Uint32Array(1);
        crypto.getRandomValues(random);
        const strokeId = random[0] || 1;
        activeStrokeRef.current = strokeId;
        sequenceRef.current = 0;
        pendingPointsRef.current = [];
        lastFlushAtRef.current = performance.now() - DRAW_FRAME_INTERVAL_MS;
        const wireColor = tool === "eraser" ? 0xffffff : Number.parseInt(color.slice(1), 16);
        const wireWidth = Math.min(64, Math.max(1, Math.round(tool === "eraser" ? size * 2 : size)));
        onDrawRef.current({ kind: "begin", strokeId, color: wireColor, width: wireWidth, start: rounded(point) });
      }
    }
  };

  const pointerMove = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d", { willReadFrequently: true });
    if (!canvas || !context || !startRef.current || !lastRef.current) return;
    const coalesced = event.nativeEvent.getCoalescedEvents?.() ?? [];
    const pointerEvents = coalesced.length > 0 ? coalesced : [event.nativeEvent];
    const reportedPoints = pointerEvents.map((pointerEvent) => canvasPoint(canvas, pointerEvent));
    const point = reportedPoints[reportedPoints.length - 1];
    if (!point) return;
    prepareBrush(context, tool, color, size);
    if (["line", "arrow", "rectangle", "ellipse", "triangle", "star"].includes(tool)) {
      if (previewRef.current) context.putImageData(previewRef.current, 0, 0);
      drawShape(context, tool, startRef.current, point);
    } else {
      let previous = lastRef.current;
      const sampledPoints: Point[] = [];
      for (const reportedPoint of reportedPoints) {
        for (const sampledPoint of sampledSegment(previous, reportedPoint)) {
          context.beginPath();
          context.moveTo(previous.x, previous.y);
          context.quadraticCurveTo(previous.x, previous.y, (previous.x + sampledPoint.x) / 2, (previous.y + sampledPoint.y) / 2);
          context.stroke();
          if (tool === "pastel") {
            context.globalAlpha = 0.22;
            for (let jitter = -2; jitter <= 2; jitter += 2) {
              context.beginPath();
              context.moveTo(previous.x + jitter, previous.y - jitter);
              context.lineTo(sampledPoint.x + jitter, sampledPoint.y - jitter);
              context.stroke();
            }
            prepareBrush(context, tool, color, size);
          }
          sampledPoints.push(sampledPoint);
          previous = sampledPoint;
        }
      }
      if (onDrawRef.current && activeStrokeRef.current !== null) {
        for (const sampledPoint of sampledPoints) {
          const wirePoint = rounded(sampledPoint);
          const previousWirePoint = pendingPointsRef.current[pendingPointsRef.current.length - 1];
          if (!previousWirePoint || previousWirePoint.x !== wirePoint.x || previousWirePoint.y !== wirePoint.y) {
            pendingPointsRef.current.push(wirePoint);
          }
        }
        if (pendingPointsRef.current.length >= 64) flushPoints();
        else if (flushTimerRef.current === null) {
          const elapsed = performance.now() - lastFlushAtRef.current;
          const delay = Math.max(0, DRAW_FRAME_INTERVAL_MS - elapsed);
          flushTimerRef.current = window.setTimeout(flushPoints, delay);
        }
      }
    }
    lastRef.current = point;
  };

  const pointerUp = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    if (canvasRef.current?.hasPointerCapture(event.pointerId)) canvasRef.current.releasePointerCapture(event.pointerId);
    const start = startRef.current;
    const end = lastRef.current;
    if (start && end && ["line", "arrow", "rectangle", "ellipse", "triangle", "star"].includes(tool) && onDrawRef.current) {
      sendWholeStroke(shapePoints(tool, start, end), color, Math.min(64, Math.max(1, Math.round(size))));
    } else if (activeStrokeRef.current !== null && onDrawRef.current) {
      flushPoints();
      onDrawRef.current({ kind: "end", strokeId: activeStrokeRef.current, sequence: sequenceRef.current });
    }
    activeStrokeRef.current = null;
    startRef.current = null;
    lastRef.current = null;
    previewRef.current = null;
    const context = canvasRef.current?.getContext("2d");
    if (context) {
      context.globalAlpha = 1;
      context.globalCompositeOperation = "source-over";
    }
  };

  const undo = () => {
    if (onDrawRef.current) {
      onDrawRef.current({ kind: "undo" });
      return;
    }
    const context = canvasRef.current?.getContext("2d", { willReadFrequently: true });
    const previous = historyRef.current.pop();
    if (!context || !previous) return;
    futureRef.current.push(context.getImageData(0, 0, CANVAS_WIDTH, CANVAS_HEIGHT));
    context.putImageData(previous, 0, 0);
    updateHistoryState();
  };

  const redo = () => {
    if (onDrawRef.current) return;
    const context = canvasRef.current?.getContext("2d", { willReadFrequently: true });
    const next = futureRef.current.pop();
    if (!context || !next) return;
    historyRef.current.push(context.getImageData(0, 0, CANVAS_WIDTH, CANVAS_HEIGHT));
    context.putImageData(next, 0, 0);
    updateHistoryState();
  };

  const clear = () => {
    if (onDrawRef.current) {
      onDrawRef.current({ kind: "clear" });
      return;
    }
    const context = canvasRef.current?.getContext("2d");
    if (!context) return;
    saveSnapshot();
    context.globalCompositeOperation = "source-over";
    context.globalAlpha = 1;
    context.fillStyle = "#ffffff";
    context.fillRect(0, 0, CANVAS_WIDTH, CANVAS_HEIGHT);
  };

  const brushTools = TOOL_GROUPS.find((group) => group.label === "Draw")?.tools ?? [];
  const shapeTools = TOOL_GROUPS.find((group) => group.label === "Shapes")?.tools ?? [];
  const panelTitle = openSection === "brushes"
    ? "Brushes"
    : openSection === "shapes"
      ? "Shapes"
      : openSection === "colors"
        ? tool === "bucket" ? "Fill color" : "Pen color"
        : "Stroke size";

  const toggleSection = (section: Exclude<ToolPanelSection, null>) => {
    setOpenSection((current) => current === section ? null : section);
  };

  return (
    <div className={`${styles.drawingStudio} ${compact ? styles.compactStudio : ""}`}>
      <div className={styles.canvasWrap}>
        <canvas
          aria-label="NDraw drawing canvas"
          className={styles.canvas}
          data-enabled={enabled}
          height={CANVAS_HEIGHT}
          onPointerCancel={pointerUp}
          onPointerDown={pointerDown}
          onPointerMove={pointerMove}
          onPointerUp={pointerUp}
          ref={canvasRef}
          width={CANVAS_WIDTH}
        />
        <div className={styles.canvasCornerLabel}>1024 × 960</div>
      </div>

      <div className={styles.toolDock}>
        <button aria-expanded={openSection === "brushes"} className={styles.quickToolButton} data-active={brushTools.some(({ id }) => id === tool)} onClick={() => toggleSection("brushes")} type="button">
          <PaintBrush size={20} weight="bold" /><span>Pen</span>
        </button>
        <button aria-expanded={openSection === "size" && tool === "eraser"} className={styles.quickToolButton} data-active={tool === "eraser"} onClick={() => { setTool("eraser"); setOpenSection("size"); }} type="button">
          <Eraser size={20} weight="bold" /><span>Eraser</span>
        </button>
        <button aria-expanded={openSection === "colors" && tool === "bucket"} className={styles.quickToolButton} data-active={tool === "bucket"} onClick={() => { setTool("bucket"); setOpenSection("colors"); }} type="button">
          <PaintBucket size={20} weight="bold" /><span>Fill</span>
        </button>
        <button aria-expanded={openSection === "shapes"} className={styles.quickToolButton} data-active={shapeTools.some(({ id }) => id === tool)} onClick={() => { if (!shapeTools.some(({ id }) => id === tool)) setTool("line"); toggleSection("shapes"); }} type="button">
          <Rectangle size={20} weight="bold" /><span>Shapes</span>
        </button>
        <button aria-expanded={openSection === "colors" && tool !== "bucket"} aria-label="Choose pen color" className={`${styles.quickToolButton} ${styles.colorQuickButton}`} onClick={() => { if (tool === "bucket") setTool("pen"); toggleSection("colors"); }} style={{ "--swatch": color } as CSSProperties} type="button">
          <span className={styles.quickColorSwatch} /><span>Color</span>
        </button>
        <button aria-expanded={openSection === "size"} className={styles.quickToolButton} onClick={() => toggleSection("size")} type="button">
          <SlidersHorizontal size={20} weight="bold" /><span>{size}px</span>
        </button>
        <span className={styles.dockDivider} />
        <button aria-label="Undo" className="icon-button" disabled={!enabled || (onDraw ? strokes.length === 0 : !canUndo)} onClick={undo} type="button"><ArrowCounterClockwise size={20} weight="bold" /></button>
        <button aria-label="Redo" className="icon-button" disabled={!enabled || Boolean(onDraw) || !canRedo} onClick={redo} type="button"><ArrowClockwise size={20} weight="bold" /></button>
        <button aria-label="Clear canvas" className="icon-button" disabled={!enabled} onClick={clear} type="button"><Trash size={19} weight="bold" /></button>
      </div>

      {openSection ? (
        <aside className={styles.toolPanel} aria-label="Drawing tools">
          <div className={styles.toolPanelHeader}>
            <div><span className={styles.eyebrow}>Drawing tool</span><strong>{panelTitle}</strong></div>
            <button aria-label="Close toolbox" className="icon-button" onClick={() => setOpenSection(null)} type="button"><X size={18} weight="bold" /></button>
          </div>
          {openSection === "brushes" || openSection === "shapes" ? (
            <div className={styles.toolGroup}>
              <span>{openSection === "brushes" ? "Choose a brush" : "Choose a shape"}</span>
              <div className={styles.toolGrid}>
                {(openSection === "brushes" ? brushTools : shapeTools).map(({ id, label, icon: Icon }) => (
                  <button
                    data-active={tool === id}
                    key={id}
                    onClick={() => setTool(id)}
                    title={label}
                    type="button"
                  >
                    <Icon size={21} weight={tool === id ? "fill" : "regular"} /><small>{label}</small>
                  </button>
                ))}
              </div>
            </div>
          ) : null}
          {openSection === "colors" ? (
            <div className={styles.toolGroup}>
              <span>{tool === "bucket" ? "Canvas background" : "Ink color"}</span>
              <div className={styles.paletteGrid}>
                {PALETTE.map((swatch) => (
                  <button aria-label={`Use ${swatch}`} data-selected={color === swatch} key={swatch} onClick={() => setColor(swatch)} style={{ background: swatch }} type="button" />
                ))}
              </div>
              {tool === "bucket" ? <small className={styles.toolHelp}>Tap the board to fill its background.</small> : null}
            </div>
          ) : null}
          {openSection === "size" ? (
            <label className={styles.sizeControl}>
              <span>{tool === "eraser" ? "Eraser size" : "Stroke size"} <b>{size}px</b></span>
              <input aria-label="Brush size" max="48" min="2" onChange={(event) => setSize(Number(event.target.value))} type="range" value={size} />
            </label>
          ) : null}
        </aside>
      ) : null}
    </div>
  );
}
