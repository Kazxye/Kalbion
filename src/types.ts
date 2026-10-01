export interface Item {
  id: string;
  name: string;
  tier: number;
  enchantment: number;
  quality: number;
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
  quantity: number;
}
export interface Price {
  unit_silver: number;
  source: string;
  server: string;
  city: string;
  queried_at: string;
}
export interface LootRow {
  event: LootEvent;
  imported: boolean;
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
export interface Filter {
  player: string;
  item: string;
  tier: number | null;
  enchantment: number | null;
  quality: number | null;
}
export interface Ledger {
  id: string;
  kind: 'income' | 'expense' | 'regear' | 'settlement';
  player: string;
  description: string;
  amount: number;
  occurred_at: string;
}
export interface View {
  session: Session;
  rows: LootRow[];
  totals: Totals;
  full_totals: Totals;
  ledger: Ledger[];
  finance: {
    income: number;
    expenses: number;
    settlements: number;
    available: number;
  };
}
export interface Bootstrap {
  sessions: Session[];
  settings: Settings;
  catalog: Item[];
  license: { state: string; reason?: string };
}
export interface Share {
  player: string;
  silver: number;
}
