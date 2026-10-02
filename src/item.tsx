import { useEffect, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { Boxes, FlaskConical, Import, PenLine, Radio } from 'lucide-react';
import { desktop } from './api';
import { qualityLabel } from './format';
import type { Item, LootEvent } from './types';

/** Just past the Rust cooldown for failed downloads (120 s), so a retry can reach the network. */
const ICON_RETRY_MS = 130_000;
function iconUrl(id: string, quality: number | null, attempt: number) {
  const params = new URLSearchParams();
  if (quality) params.set('quality', String(quality));
  // A new URL per attempt keeps the webview from reusing a cached failure.
  if (attempt) params.set('attempt', String(attempt));
  const query = params.toString();
  return `${convertFileSrc(id, 'icon')}${query ? `?${query}` : ''}`;
}

/**
 * Official render served through the Rust icon cache. On failure it shows a generic icon
 * and retries in the background with an off-screen probe, swapping only if it loads.
 */
export function ItemIcon({
  item,
  quality,
}: {
  item: Item;
  quality: number | null;
}) {
  const key = `${item.id}:${quality ?? ''}`;
  const [state, setState] = useState({ key, attempt: 0, failed: false });
  const current =
    state.key === key ? state : { key, attempt: 0, failed: false };
  useEffect(() => {
    if (!desktop || !current.failed) return;
    let probe: HTMLImageElement | undefined;
    const timer = setTimeout(() => {
      const attempt = current.attempt + 1;
      probe = new Image();
      probe.onload = () => setState({ key, attempt, failed: false });
      probe.onerror = () => setState({ key, attempt, failed: true });
      probe.src = iconUrl(item.id, quality, attempt);
    }, ICON_RETRY_MS);
    return () => {
      clearTimeout(timer);
      if (probe) probe.onload = probe.onerror = null;
    };
  }, [key, current.attempt, current.failed, item.id, quality]);
  const showImage = desktop && !current.failed;
  return (
    <span className={`item-icon ${showImage ? 'with-image' : ''}`}>
      {showImage ? (
        <img
          src={iconUrl(item.id, quality, current.attempt)}
          alt=""
          loading="lazy"
          decoding="async"
          onError={() =>
            setState({ key, attempt: current.attempt, failed: true })
          }
        />
      ) : (
        <Boxes size={20} aria-hidden />
      )}
      <small aria-hidden>{item.tier ? `T${item.tier}` : '—'}</small>
    </span>
  );
}

/** "T6.2" with the game's tier and enchantment colors; the text carries the meaning. */
export function TierBadge({ item }: { item: Item }) {
  const label =
    item.tier === null
      ? `Sem tier, encantamento ${item.enchantment}`
      : `Tier ${item.tier}, encantamento ${item.enchantment}`;
  return (
    <span
      className="tier-badge"
      data-tier={item.tier ?? 0}
      data-enchantment={item.enchantment}
      title={label}
    >
      <span aria-hidden className="t">
        {item.tier === null ? '—' : `T${item.tier}`}
      </span>
      <span aria-hidden className="e">
        .{item.enchantment}
      </span>
      <span className="visually-hidden">{label}</span>
    </span>
  );
}

/** Filled diamonds for quality 1–5 plus its name; unknown quality is never drawn as Normal. */
export function QualityMark({
  quality,
  hasQuality,
}: {
  quality: number | null;
  hasQuality: boolean;
}) {
  if (!hasQuality)
    return <span className="quality not-applicable">Não se aplica</span>;
  return (
    <span className={`quality ${quality === null ? 'unknown' : ''}`}>
      <span className="pips" aria-hidden>
        {[1, 2, 3, 4, 5].map((level) => (
          <i
            key={level}
            className={quality !== null && level <= quality ? 'on' : ''}
          />
        ))}
      </span>
      {qualityLabel(quality)}
    </span>
  );
}

const origins = {
  simulated: { label: 'Simulado', icon: FlaskConical },
  manual: { label: 'Manual', icon: PenLine },
  observed: { label: 'Observado (declarado)', icon: Radio },
} as const;
export function OriginTag({
  origin,
  imported,
}: {
  origin: LootEvent['origin'];
  imported: boolean;
}) {
  const { icon: Icon } = origins[origin];
  // Observed and not imported: Kalbion itself captured it (live capture).
  const label =
    origin === 'observed' && !imported
      ? 'Capturado ao vivo'
      : origins[origin].label;
  return (
    <span className="origin-tags">
      <span className={`tag ${origin === 'simulated' ? 'simulated' : ''}`}>
        <Icon size={12} aria-hidden />
        {label}
      </span>
      {imported && (
        <span className="tag">
          <Import size={12} aria-hidden />
          Importado
        </span>
      )}
    </span>
  );
}
