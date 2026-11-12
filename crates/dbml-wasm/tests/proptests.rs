//! Testes baseados em propriedade (proptest) para o `LineIndex` e gerador de patches mínimos.

use dbml_wasm::compute_minimal_patch;
use dbml_wasm::line_index::LineIndex;
use proptest::prelude::*;

proptest! {
    /// Invariância: O patch mínimo aplicado sobre o texto original deve sempre reproduzir
    /// exatamente o texto atualizado, independentemente do tamanho e dos caracteres.
    #[test]
    fn test_patch_reconstruction_invariance(
        orig in "\\PC*",
        mod_prefix in "\\PC*",
        mod_suffix in "\\PC*"
    ) {
        let modified = format!("{mod_prefix}{orig}{mod_suffix}");
        let line_index = LineIndex::new(&orig);
        let patch = compute_minimal_patch(&orig, &modified, &line_index);

        let mut reconstructed = orig.clone();
        reconstructed.replace_range(patch.byte_start..patch.byte_end, &patch.new_text);

        prop_assert_eq!(reconstructed, modified);
    }

    /// Invariância: Para qualquer string contendo caracteres variados (inclusive acentos e quebras de linha),
    /// a conversão de offset para posição e de volta para offset deve ser idempotente.
    #[test]
    fn test_line_index_roundtrip(text in "[a-zA-Z0-9 áéíóúãõçÁÉÍÓÚÃÕÇ\r\n]{0,200}") {
        let idx = LineIndex::new(&text);

        for (offset, _) in text.char_indices() {
            let pos = idx.offset_to_position(&text, offset);
            let back = idx.position_to_offset(&text, pos);
            prop_assert_eq!(back, Some(offset));
        }
    }
}
