/**
 * Relight's lights and the Vignette and Centre pins, on the photo. Ours.
 *
 * The shapes are RapidRAW's (overlays/RelightLightShape.tsx, 68b14a36, and
 * overlays/CenterMarkerShape.tsx, b39884b3), drawn by ImageCanvas.tsx on its
 * Konva stage as theirs does. What is ours is when they show, and three
 * additions from the 2026-10-20 placement note: each light carries its row
 * number and a right-click menu, and a pin carries its name, with the outline
 * of what it moves only while it is dragged.
 *
 * One rule for both (their behaviour, 49bb69a5 and b39884b3): a tool's handles
 * show while you work on it, and go when you click anything else or press Esc.
 * Only one tool's at a time: the lights, a pin, or the white balance picker.
 * They show only on the Edit tab with their part open: Effects (and its
 * Relight or Vignette part) for the lights and the vignette pin, Basic for
 * Centre, which Argentum moved there from Details.
 */

import { useEffect, useRef, useState } from 'react';
import { Group, Text as KonvaText } from 'react-konva';
import type { KonvaEventObject } from 'konva/lib/Node';
import CenterMarkerShape from '../../components/panel/editor/overlays/CenterMarkerShape';
import RelightLightShape, { type RelightBasis } from '../../components/panel/editor/overlays/RelightLightShape';
import { Panel } from '../../components/ui/AppProperties';
import { type CenterMarker, useEditorStore } from '../../store/useEditorStore';
import { useSettingsStore } from '../../store/useSettingsStore';
import { useUIStore } from '../../store/useUIStore';
import type { Coord, RelightLight } from '../../utils/adjustments';
import { useAgTranslation } from '../locales';

/** The Edit section each pin's sliders are in, and the part within it, if any. */
const PIN_PLACE: Record<CenterMarker, { section: 'basic' | 'effects'; part: string | null }> = {
  vignette: { section: 'effects', part: 'vignette' },
  centre: { section: 'basic', part: null },
};

/** What a pointer press may land on without putting the handles away. */
const KEEPS_HANDLES = '.konvajs-content, [role="menu"]';

interface PhotoState {
  isMasking: boolean;
  isAiEditing: boolean;
  isCropping: boolean;
  showOriginal: boolean;
  isWbPickerActive: boolean;
  relightOn: boolean;
  setIsMaskHovered(isHovered: boolean): void;
}

/** Which handles the photo shows now, and the rules that put them away. */
export function useOnPhotoHandles({
  isMasking,
  isAiEditing,
  isCropping,
  showOriginal,
  isWbPickerActive,
  relightOn,
  setIsMaskHovered,
}: PhotoState) {
  const setEditor = useEditorStore((s) => s.setEditor);
  const isLightsActive = useEditorStore((s) => s.isRelightPickerActive);
  const pin = useEditorStore((s) => s.activeCenterMarker);
  const editOpen = useUIStore((s) => s.activeView === 'editor' && s.activePanel === Panel.Adjustments);
  const sections = useUIStore((s) => s.collapsibleSectionsState);
  const collapsedParts = useSettingsStore((s) => s.appSettings?.adjustmentLayout?.collapsedTools);

  const partOpen = (part: string | null) => !part || !(collapsedParts ?? []).includes(part);
  const lightsOpen = editOpen && sections.effects && partOpen('relight') && relightOn;
  const pinOpen = !!pin && editOpen && sections[PIN_PLACE[pin].section] && partOpen(PIN_PLACE[pin].part);
  const free = !isMasking && !isAiEditing && !isCropping;

  const showLights = isLightsActive && lightsOpen && free;
  const showPin = !!pin && pinOpen && free && !showOriginal;

  // Folded away, another tab, Relight off: the handles go with them.
  useEffect(() => {
    if (isLightsActive && !lightsOpen) setEditor({ isRelightPickerActive: false });
  }, [isLightsActive, lightsOpen, setEditor]);
  useEffect(() => {
    if (pin && !pinOpen) setEditor({ activeCenterMarker: null });
  }, [pin, pinOpen, setEditor]);

  // The white balance picker puts both away.
  useEffect(() => {
    if (isWbPickerActive) setEditor({ isRelightPickerActive: false, activeCenterMarker: null });
  }, [isWbPickerActive, setEditor]);

  // A press anywhere but the photo, a menu, or the controls of what is showing.
  useEffect(() => {
    if (!isLightsActive && !pin) return;
    const keeps = isLightsActive ? '[data-relight-lights]' : `[data-center-marker-controls="${pin}"]`;
    const onPointerDown = (e: PointerEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.closest(`${KEEPS_HANDLES}, ${keeps}`)) return;
      setEditor({ isRelightPickerActive: false, activeCenterMarker: null });
    };
    window.addEventListener('pointerdown', onPointerDown, true);
    return () => window.removeEventListener('pointerdown', onPointerDown, true);
  }, [isLightsActive, pin, setEditor]);

  // Esc puts them away before it does anything else.
  const shown = showLights || showPin;
  useEffect(() => {
    if (!shown) return;
    const ui = useUIStore.getState();
    const before = ui.customEscapeHandler;
    const handler = () => setEditor({ isRelightPickerActive: false, activeCenterMarker: null });
    ui.setCustomEscapeHandler(handler);
    return () => {
      const now = useUIStore.getState();
      if (now.customEscapeHandler === handler) now.setCustomEscapeHandler(before);
    };
  }, [shown, setEditor]);

  // A handle that vanishes under the pointer never reports leaving it, which
  // would leave the photo unable to pan.
  const wasShown = useRef(false);
  useEffect(() => {
    if (wasShown.current && !shown) setIsMaskHovered(false);
    wasShown.current = shown;
  }, [shown, setIsMaskHovered]);

  return { showLights, showPin, pin };
}

interface HandleEvents {
  zoomScale: number;
  onDragStateChange(isDragging: boolean): void;
  onHoverChange(isHovered: boolean): void;
  onTouchInteraction(): void;
}

interface RelightHandlesProps extends HandleEvents {
  lights: RelightLight[];
  activeId: string | null;
  basis: RelightBasis;
  positionOf(light: RelightLight): Coord;
  onSelect(id: string): void;
  onMove(id: string, pos: Coord): void;
  onAim(id: string, angle: number, elevation: number): void;
  onMenu(id: string, x: number, y: number): void;
}

/** Each light with its row number; the selected one drawn last, over the others. */
export function RelightHandles({
  lights,
  activeId,
  basis,
  positionOf,
  zoomScale,
  onSelect,
  onMove,
  onAim,
  onMenu,
  onDragStateChange,
  onHoverChange,
  onTouchInteraction,
}: RelightHandlesProps) {
  const px = (screenPx: number) => screenPx / zoomScale;
  const ordered = [...lights.filter((l) => l.id !== activeId), ...lights.filter((l) => l.id === activeId)];

  return (
    <>
      {ordered.map((light) => {
        const pos = positionOf(light);
        return (
          <Group
            key={light.id}
            onContextMenu={(e: KonvaEventObject<PointerEvent>) => {
              // Not the photo's own menu, which their container opens.
              e.evt.preventDefault();
              e.evt.stopPropagation();
              e.cancelBubble = true;
              onMenu(light.id, e.evt.clientX, e.evt.clientY);
            }}
          >
            <RelightLightShape
              light={light}
              pos={pos}
              basis={basis}
              zoomScale={zoomScale}
              isActive={light.id === activeId}
              onSelect={() => onSelect(light.id)}
              onMove={(p) => onMove(light.id, p)}
              onAim={(angle, elevation) => onAim(light.id, angle, elevation)}
              onDragStateChange={onDragStateChange}
              onHoverChange={onHoverChange}
              onTouchInteraction={onTouchInteraction}
            />
            <KonvaText
              x={pos.x + px(10)}
              y={pos.y - px(22)}
              text={String(lights.indexOf(light) + 1)}
              fontSize={px(12)}
              fontStyle="bold"
              fill="white"
              shadowColor="black"
              shadowBlur={3}
              shadowOpacity={0.8}
              listening={false}
            />
          </Group>
        );
      })}
    </>
  );
}

interface CentrePinProps extends HandleEvents {
  pin: CenterMarker;
  pos: Coord;
  imageWidth: number;
  imageHeight: number;
  midpoint: number;
  roundness: number;
  onMove(pos: Coord): void;
  onReset(): void;
}

/** Their pin, named, with the outline of its effect only while it is dragged. */
export function CentrePin({
  pin,
  pos,
  imageWidth,
  imageHeight,
  midpoint,
  roundness,
  zoomScale,
  onMove,
  onReset,
  onDragStateChange,
  onHoverChange,
  onTouchInteraction,
}: CentrePinProps) {
  const t = useAgTranslation();
  const [isDragging, setIsDragging] = useState(false);
  const px = (screenPx: number) => screenPx / zoomScale;
  const textProps = {
    fill: 'white',
    shadowColor: 'black',
    shadowBlur: 3,
    shadowOpacity: 0.8,
    listening: false,
  };

  return (
    <>
      <CenterMarkerShape
        pos={pos}
        // Their shape draws the outline whenever it has the image size.
        imageWidth={isDragging ? imageWidth : 0}
        imageHeight={isDragging ? imageHeight : 0}
        midpoint={midpoint}
        roundness={roundness}
        zoomScale={zoomScale}
        onMove={onMove}
        onReset={onReset}
        onDragStateChange={(dragging) => {
          setIsDragging(dragging);
          onDragStateChange(dragging);
        }}
        onHoverChange={onHoverChange}
        onTouchInteraction={onTouchInteraction}
      />
      <KonvaText
        {...textProps}
        x={pos.x + px(20)}
        y={pos.y - px(15)}
        text={t(pin === 'vignette' ? 'fx166VignetteCentre' : 'fx166Centre')}
        fontSize={px(12)}
        fontStyle="bold"
      />
      <KonvaText
        {...textProps}
        x={pos.x + px(20)}
        y={pos.y + px(1)}
        text={t('fx166PinHint')}
        fontSize={px(11)}
        opacity={0.85}
      />
    </>
  );
}
