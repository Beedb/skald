/** Barra de categorias da biblioteca (seção 10). Por ora só Música está ativa. */
const CATEGORIAS = [
  { id: "music", nome: "Música", ativa: true },
  { id: "podcast", nome: "Podcasts", ativa: false },
  { id: "audiobook", nome: "Audiolivros", ativa: false },
  { id: "other", nome: "Outros áudios", ativa: false },
];

export default function Categorias() {
  return (
    <nav className="categorias" aria-label="Categorias">
      {CATEGORIAS.map((c) => (
        <button
          key={c.id}
          className={c.id === "music" ? "ativa" : undefined}
          aria-pressed={c.id === "music"}
          disabled={!c.ativa}
          title={c.ativa ? undefined : "Em breve"}
        >
          {c.nome}
        </button>
      ))}
    </nav>
  );
}
