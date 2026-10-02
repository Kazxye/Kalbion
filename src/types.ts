export interface Item {
  id: string;
  name: string;
  tier: number | null;
  enchantment: number;
  /** False for resources, which exist in a single quality ("não se aplica"). */
  has_quality: boolean;
}
export interface Session {
  id: string;
  name: string;
  created_at: string;
  closed_at: string | null;
  server: string;
  city: string;
}
export interface Settings {
  server: string;
  city: string;
}
export interface LootEvent {
  id: string;
  source: string;
  origin: 'simulated' | 'manual' | 'observed';
  session_id: string;
  occurred_at: string;
  player: string;
  item: Item;
  quality: number | null;
  quantity: number;
}
/** A manual price always prevails over the Albion Data Project for the same item and quality. */
export type PriceSource = 'manual' | 'albion_data';
export interface Price {
  unit_silver: number;
  source: PriceSource;
  server: string;
  city: string;
  recorded_at: string;
  observed_at: string | null;
}
export interface LootRow {
  event: LootEvent;
  imported: boolean;
  voided_at: string | null;
  price: Price | null;
}
export interface Total {
  events: number;
  quantity: number;
  estimated_silver: number;
  unpriced_events: number;
}
export interface Totals {
  session: Total;
  players: Record<string, Total>;
}
/** quality 0 selects events whose quality is unknown. */
export interface Filter {
  player: string;
  item: string;
  tier: number | null;
  enchantment: number | null;
  quality: number | null;
}
export type LedgerKind = 'income' | 'expense' | 'regear' | 'settlement';
export interface Ledger {
  id: string;
  kind: LedgerKind;
  player: string;
  description: string;
  amount: number;
  occurred_at: string;
  reverses: string | null;
  reversed_by: string | null;
}
export interface Finance {
  income: number;
  expenses: number;
  settlements: number;
  available: number;
}
export interface View {
  session: Session;
  rows: LootRow[];
  totals: Totals;
  full_totals: Totals;
  ledger: Ledger[];
  finance: Finance;
}
export interface CatalogInfo {
  kind: 'builtin' | 'ao_bin_dumps';
  label: string;
  imported_at: string | null;
  item_count: number;
  skipped_count: number;
}
export interface License {
  state: 'disabled' | 'unauthenticated' | 'valid' | 'expired' | 'unavailable';
  reason?: string;
  expires_at?: string;
}
export interface Bootstrap {
  sessions: Session[];
  settings: Settings;
  catalog: CatalogInfo;
  license: License;
  log_path: string | null;
  /** True once writing the log file has failed; details went to stderr. */
  log_failed: boolean;
}
export interface Share {
  player: string;
  silver: number;
}
export interface CaptureSummary {
  file_label: string;
  inserted: number;
  duplicates: number;
  report: {
    loot_observed: number;
    converted: number;
    outside_roster: number;
    unknown_items: Record<string, number>;
    invalid_players: number;
  };
  diagnostics: {
    decoder_version: string;
    codebook_observed_until: string;
    file_truncated: boolean;
    loot_events: number;
    loot_silver: number;
    loot_malformed: number;
  };
  /** Captured after the latest date the decoder's event codes were observed. */
  newer_than_codebook: boolean;
}
export interface MarketRefresh {
  updated: number;
  unavailable: number;
  manual_kept: number;
  unknown_quality: number;
}
export interface InsertResult {
  inserted: number;
  duplicates: number;
}
export interface LiveInterface {
  name: string;
  description: string | null;
  addresses: string[];
  loopback: boolean;
  up: boolean;
  running: boolean;
}
export type LiveOutcome =
  | 'inserted'
  | 'duplicate'
  | 'outside_roster'
  | 'unknown_item'
  | 'invalid_player'
  | 'error';
export interface RecentLoot {
  at: string;
  /** Only players on the roster are named. */
  player: string | null;
  item_index: number;
  item: Item | null;
  quantity: number;
  outcome: LiveOutcome;
}
export interface LiveDiagnostics {
  packets: number;
  udp_from_game_server: number;
  photon_packets: number;
  encrypted_packets: number;
  encrypted_messages: number;
  events: number;
  events_undecodable: number;
  loot_events: number;
  loot_silver: number;
  loot_malformed: number;
  fragments_dropped: number;
}
export interface LiveStatus {
  state: 'idle' | 'running' | 'stopped' | 'failed';
  session_id: string | null;
  interface: string | null;
  started_at: string | null;
  stopped_at: string | null;
  error: string | null;
  inserted: number;
  duplicates: number;
  outside_roster: number;
  invalid_players: number;
  unknown_items: Record<string, number>;
  dropped_by_capture: number;
  recent: RecentLoot[];
  trace_path: string | null;
  trace_full: boolean;
  diagnostics: LiveDiagnostics | null;
}
