export interface Item {
  id: string;
  name: string;
  tier: number | null;
  enchantment: number;
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
export interface Price {
  unit_silver: number;
  source: 'manual';
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
}
export interface Share {
  player: string;
  silver: number;
}
export interface InsertResult {
  inserted: number;
  duplicates: number;
}
