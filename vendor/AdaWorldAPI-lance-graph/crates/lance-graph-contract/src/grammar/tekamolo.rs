//! TEKAMOLO — Temporal / Kausal / Modal / Lokal adverbial slots.
//!
//! German grammar mnemonic, universally applicable. Every sentence has
//! up to four adverbial slots answering: when / why / how / where.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TekamoloSlot {
    Temporal,   // when
    Kausal,     // why / because
    Modal,      // how / in what manner
    Lokal,      // where
    Instrument, // by what means / with what
}

/// Slot fillers as lightweight token-index pairs. Downstream crates carry
/// the actual strings; the contract only carries positions.
#[derive(Debug, Clone, Default)]
pub struct TekamoloSlots {
    pub temporal: Option<(u16, u16)>,
    pub kausal: Option<(u16, u16)>,
    pub modal: Option<(u16, u16)>,
    pub lokal: Option<(u16, u16)>,
}

impl TekamoloSlots {
    pub fn filled_count(&self) -> u8 {
        let mut n = 0;
        if self.temporal.is_some() {
            n += 1;
        }
        if self.kausal.is_some() {
            n += 1;
        }
        if self.modal.is_some() {
            n += 1;
        }
        if self.lokal.is_some() {
            n += 1;
        }
        n
    }

    pub fn is_empty(&self) -> bool {
        self.filled_count() == 0
    }
}
