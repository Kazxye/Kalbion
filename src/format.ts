export const qualities = [
  'Normal',
  'Bom',
  'Excepcional',
  'Excelente',
  'Obra-prima',
];
export const qualityLabel = (quality: number | null) =>
  quality === null ? 'Desconhecida' : qualities[quality - 1];
export const tierLabel = (tier: number | null, enchantment: number) =>
  tier === null ? `—.${enchantment}` : `T${tier}.${enchantment}`;
export const serverNames: Record<string, string> = {
  americas: 'Américas',
  europe: 'Europa',
  asia: 'Ásia',
};
export const cities = [
  'Bridgewatch',
  'Martlock',
  'Lymhurst',
  'Fort Sterling',
  'Thetford',
  'Caerleon',
  'Brecilien',
];
export const ledgerKinds: Record<string, string> = {
  income: 'Receita recebida',
  expense: 'Despesa',
  regear: 'Regear',
  settlement: 'Acerto pago',
};
export const silver = (amount: number) =>
  new Intl.NumberFormat('pt-BR').format(amount);
export const date = (value: string) =>
  new Date(value).toLocaleString('pt-BR', {
    dateStyle: 'short',
    timeStyle: 'short',
  });
