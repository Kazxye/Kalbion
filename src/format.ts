export const qualities = [
  'Normal',
  'Bom',
  'Excepcional',
  'Excelente',
  'Obra-prima',
];
export const qualityLabel = (quality: number | null, hasQuality = true) =>
  !hasQuality
    ? 'Não se aplica'
    : quality === null
      ? 'Desconhecida'
      : qualities[quality - 1];
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
const hour = 3_600_000;
/** Relative age such as "há 5 min"; market quotes are judged by how old they are. */
export function age(value: string) {
  const minutes = Math.max(
    0,
    Math.floor((Date.now() - new Date(value).getTime()) / 60_000),
  );
  if (minutes < 1) return 'agora';
  if (minutes < 60) return `há ${minutes} min`;
  const hours = Math.floor(minutes / 60);
  return hours < 48 ? `há ${hours} h` : `há ${Math.floor(hours / 24)} d`;
}
export const isStale = (value: string) =>
  Date.now() - new Date(value).getTime() > 24 * hour;
const plural = (count: number, one: string, many: string) =>
  `${count} ${count === 1 ? one : many}`;
export function marketSummary(result: {
  updated: number;
  unavailable: number;
  manual_kept: number;
  unknown_quality: number;
}) {
  const parts = [
    result.updated + result.unavailable === 0
      ? 'Nenhum item para consultar no Albion Data Project'
      : `Albion Data Project: ${plural(result.updated, 'preço atualizado', 'preços atualizados')}`,
  ];
  if (result.unavailable)
    parts.push(
      plural(
        result.unavailable,
        'item sem oferta à venda',
        'itens sem oferta à venda',
      ),
    );
  if (result.manual_kept)
    parts.push(
      plural(
        result.manual_kept,
        'preço manual mantido',
        'preços manuais mantidos',
      ),
    );
  if (result.unknown_quality)
    parts.push(
      `${plural(result.unknown_quality, 'item', 'itens')} com qualidade desconhecida (só preço manual)`,
    );
  return `${parts.join('; ')}.`;
}
