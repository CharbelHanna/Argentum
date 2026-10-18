/**
 * A drop-down menu for the Modern top bar: a button, and a list under it that
 * closes on a choice, a click outside, or Esc. Ours.
 */

import { type ReactNode, useEffect, useRef, useState } from 'react';
import { Check, ChevronDown } from 'lucide-react';

export function Menu({
  label,
  title,
  children,
}: {
  label: ReactNode;
  title?: string;
  children: (close: () => void) => ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const outside = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    const esc = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', outside);
    window.addEventListener('keydown', esc, true);
    return () => {
      document.removeEventListener('mousedown', outside);
      window.removeEventListener('keydown', esc, true);
    };
  }, [open]);

  return (
    <div className="agm-menu-wrap" ref={ref}>
      <button className="agm-btn" data-tooltip={title} onClick={() => setOpen((o) => !o)}>
        {label}
        <ChevronDown size={12} />
      </button>
      {open && <div className="agm-menu">{children(() => setOpen(false))}</div>}
    </div>
  );
}

export function MenuHead({ children }: { children: ReactNode }) {
  return <div className="agm-menu-head">{children}</div>;
}

export function MenuSep() {
  return <div className="agm-menu-sep" />;
}

export function MenuItem({
  checked,
  onPick,
  shortcut,
  children,
}: {
  checked?: boolean;
  onPick: () => void;
  shortcut?: string;
  children: ReactNode;
}) {
  return (
    <button className="agm-menu-item" onClick={onPick}>
      <span className="agm-check">{checked ? <Check size={13} /> : null}</span>
      {children}
      {shortcut && <span className="agm-key">{shortcut}</span>}
    </button>
  );
}
