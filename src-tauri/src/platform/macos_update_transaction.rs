//! Bounded install transaction, deliberately not connected to the update UI.
//! The future caller must validate the publisher signature and complete bundle,
//! bind owner confirmation/backup, and observe the exact old process exit first.
//! This leaf never authorizes, launches, terminates, extracts or deletes anything.
use serde::Serialize;

#[cfg(target_os = "macos")]
pub(crate) mod native;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Identity {
    pub device: u64,
    pub inode: u64,
    pub uid: u32,
    pub permissions: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Layout {
    pub parent: Identity,
    pub workspace: Identity,
    pub current: Identity,
    pub candidate: Identity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Binding {
    transaction_id: String,
    from_version: String,
    to_version: String,
    archive_sha256: String,
    original: Layout,
}

impl Binding {
    pub(crate) fn new(
        transaction_id: String,
        from_version: String,
        to_version: String,
        archive_sha256: String,
        original: Layout,
        uid: u32,
    ) -> Result<Self, String> {
        let id =
            uuid::Uuid::parse_str(&transaction_id).map_err(|_| "MAC_UPDATE_INVALID_BINDING")?;
        let old =
            semver::Version::parse(&from_version).map_err(|_| "MAC_UPDATE_INVALID_BINDING")?;
        let new = semver::Version::parse(&to_version).map_err(|_| "MAC_UPDATE_INVALID_BINDING")?;
        if id.get_version_num() != 4
            || id.to_string() != transaction_id
            || new <= old
            || uid == 0
            || archive_sha256.len() != 64
            || !archive_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("MAC_UPDATE_INVALID_BINDING".into());
        }
        let nodes = [
            original.parent,
            original.workspace,
            original.current,
            original.candidate,
        ];
        if nodes.iter().any(|n| {
            n.inode == 0 || n.device != original.parent.device || n.permissions & 0o7000 != 0
        }) || (original.parent.uid != 0 && original.parent.uid != uid)
            || original.parent.permissions & 0o002 != 0
            || [original.workspace, original.current, original.candidate]
                .iter()
                .any(|n| n.uid != uid)
            || original.workspace.permissions & 0o777 != 0o700
            || [original.current, original.candidate]
                .iter()
                .any(|n| n.permissions & 0o500 != 0o500)
            || nodes
                .iter()
                .enumerate()
                .any(|(i, a)| nodes[..i].iter().any(|b| a.inode == b.inode))
        {
            return Err("MAC_UPDATE_UNTRUSTED_LAYOUT".into());
        }
        Ok(Self {
            transaction_id,
            from_version,
            to_version,
            archive_sha256,
            original,
        })
    }
    fn swapped(&self) -> Layout {
        Layout {
            current: self.original.candidate,
            candidate: self.original.current,
            ..self.original.clone()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Phase {
    Intent,
    Swapped,
    Verified,
    RollbackIntent,
    RolledBack,
}

pub(crate) trait Disk {
    fn snapshot(&mut self) -> Result<Layout, String>;
    /// INTENT creates the application-wide durable fence and a fresh journal.
    /// Existing/partial intent fences must also block a different transaction ID.
    fn record(&mut self, binding: &Binding, phase: Phase) -> Result<(), String>;
    /// One atomic same-volume exchange. An error may still have changed the namespace.
    fn swap(&mut self) -> Result<(), String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verification {
    Verified,
    Rejected,
    Unavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Inspection {
    Original,
    SwappedWithBackup,
    ForeignOrUnavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    RejectedBeforeMutation,
    NotInstalled,
    AwaitingRestartWithBackup,
    RolledBack,
    NeedsInspection,
}

/// Read-only reconciliation: never retries, rolls back, launches or removes a bundle.
pub(crate) fn inspect(binding: &Binding, disk: &mut impl Disk) -> Inspection {
    match disk.snapshot() {
        Ok(layout) if layout == binding.original => Inspection::Original,
        Ok(layout) if layout == binding.swapped() => Inspection::SwappedWithBackup,
        _ => Inspection::ForeignOrUnavailable,
    }
}

pub(crate) fn apply(
    binding: &Binding,
    disk: &mut impl Disk,
    mut verify_installed: impl FnMut() -> Verification,
) -> Outcome {
    if inspect(binding, disk) != Inspection::Original {
        return Outcome::RejectedBeforeMutation;
    }
    if disk.record(binding, Phase::Intent).is_err() {
        return Outcome::NeedsInspection;
    }
    // Durable writes can take time. Recheck before each namespace mutation.
    if inspect(binding, disk) != Inspection::Original {
        return Outcome::NeedsInspection;
    }
    if disk.swap().is_err() {
        return if inspect(binding, disk) == Inspection::Original {
            Outcome::NotInstalled
        } else {
            Outcome::NeedsInspection
        };
    }
    if inspect(binding, disk) != Inspection::SwappedWithBackup
        || disk.record(binding, Phase::Swapped).is_err()
    {
        return Outcome::NeedsInspection;
    }
    match verify_installed() {
        Verification::Unavailable => Outcome::NeedsInspection,
        Verification::Verified => {
            if inspect(binding, disk) != Inspection::SwappedWithBackup
                || disk.record(binding, Phase::Verified).is_err()
                || inspect(binding, disk) != Inspection::SwappedWithBackup
            {
                Outcome::NeedsInspection
            } else {
                Outcome::AwaitingRestartWithBackup
            }
        }
        Verification::Rejected => {
            if inspect(binding, disk) != Inspection::SwappedWithBackup
                || disk.record(binding, Phase::RollbackIntent).is_err()
                || inspect(binding, disk) != Inspection::SwappedWithBackup
            {
                return Outcome::NeedsInspection;
            }
            if disk.swap().is_err() {
                // Observed restoration does not prove a failed flush was durable.
                // Leave the application fence and backup for read-only inspection.
                return Outcome::NeedsInspection;
            }
            if inspect(binding, disk) != Inspection::Original
                || disk.record(binding, Phase::RolledBack).is_err()
                || inspect(binding, disk) != Inspection::Original
            {
                Outcome::NeedsInspection
            } else {
                Outcome::RolledBack
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn identity(inode: u64) -> Identity {
        Identity {
            device: 1,
            inode,
            uid: 501,
            permissions: 0o755,
        }
    }
    fn binding() -> Binding {
        let layout = Layout {
            parent: identity(1),
            workspace: Identity {
                permissions: 0o700,
                ..identity(2)
            },
            current: identity(3),
            candidate: identity(4),
        };
        Binding::new(
            uuid::Uuid::new_v4().to_string(),
            "0.1.0".into(),
            "0.1.1".into(),
            "a".repeat(64),
            layout,
            501,
        )
        .unwrap()
    }
    struct FakeDisk {
        layout: Layout,
        phases: Vec<Phase>,
        swaps: usize,
        journal_exists: bool,
        fail_phase: Option<Phase>,
        swap_failure: Option<(usize, bool)>,
        mutate_after_record: Option<Phase>,
        unavailable: bool,
    }
    impl FakeDisk {
        fn new(b: &Binding) -> Self {
            Self {
                layout: b.original.clone(),
                phases: vec![],
                swaps: 0,
                journal_exists: false,
                fail_phase: None,
                swap_failure: None,
                mutate_after_record: None,
                unavailable: false,
            }
        }
    }
    impl Disk for FakeDisk {
        fn snapshot(&mut self) -> Result<Layout, String> {
            if self.unavailable {
                Err("unavailable".into())
            } else {
                Ok(self.layout.clone())
            }
        }
        fn record(&mut self, _: &Binding, phase: Phase) -> Result<(), String> {
            if phase == Phase::Intent {
                if self.journal_exists {
                    return Err("existing journal".into());
                }
                self.journal_exists = true;
            }
            self.phases.push(phase);
            if self.mutate_after_record == Some(phase) {
                self.layout.current.inode = 99;
            }
            if self.fail_phase == Some(phase) {
                Err("uncertain durable write".into())
            } else {
                Ok(())
            }
        }
        fn swap(&mut self) -> Result<(), String> {
            self.swaps += 1;
            let fail = self.swap_failure.filter(|(n, _)| *n == self.swaps);
            if fail.is_none() || fail.unwrap().1 {
                std::mem::swap(&mut self.layout.current, &mut self.layout.candidate);
            }
            if fail.is_some() {
                Err("uncertain syscall/flush".into())
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn verified_exchange_retains_old_bundle_until_restart() {
        let b = binding();
        let mut d = FakeDisk::new(&b);
        assert_eq!(
            apply(&b, &mut d, || Verification::Verified),
            Outcome::AwaitingRestartWithBackup
        );
        assert_eq!(d.layout, b.swapped());
        assert_eq!(d.swaps, 1);
        assert_eq!(
            d.phases,
            vec![Phase::Intent, Phase::Swapped, Phase::Verified]
        );
        assert_eq!(inspect(&b, &mut d), Inspection::SwappedWithBackup);
        assert_eq!(d.swaps, 1);
    }
    #[test]
    fn definite_verification_rejection_rolls_back_once() {
        let b = binding();
        let mut d = FakeDisk::new(&b);
        assert_eq!(
            apply(&b, &mut d, || Verification::Rejected),
            Outcome::RolledBack
        );
        assert_eq!(d.layout, b.original);
        assert_eq!(d.swaps, 2);
        assert_eq!(
            d.phases,
            vec![
                Phase::Intent,
                Phase::Swapped,
                Phase::RollbackIntent,
                Phase::RolledBack
            ]
        );
        assert_eq!(
            apply(&b, &mut d, || panic!("no replay")),
            Outcome::NeedsInspection
        );
        assert_eq!(d.swaps, 2);
    }
    #[test]
    fn unavailable_verification_never_rolls_back_or_replays() {
        let b = binding();
        let mut d = FakeDisk::new(&b);
        assert_eq!(
            apply(&b, &mut d, || Verification::Unavailable),
            Outcome::NeedsInspection
        );
        assert_eq!(d.layout, b.swapped());
        assert_eq!(d.swaps, 1);
        assert_eq!(
            apply(&b, &mut d, || panic!("no replay")),
            Outcome::RejectedBeforeMutation
        );
        assert_eq!(d.swaps, 1);
    }

    #[test]
    fn an_unsettled_application_fence_blocks_even_a_fresh_transaction_id() {
        let b = binding();
        let mut d = FakeDisk::new(&b);
        d.journal_exists = true;
        let mut next = b.clone();
        next.transaction_id = uuid::Uuid::new_v4().to_string();
        assert_eq!(
            apply(&next, &mut d, || panic!("unsettled application")),
            Outcome::NeedsInspection
        );
        assert_eq!(d.swaps, 0);
        assert_eq!(d.layout, b.original);
    }
    #[test]
    fn journal_failures_stop_before_any_later_mutation() {
        for (phase, swaps) in [
            (Phase::Intent, 0),
            (Phase::Swapped, 1),
            (Phase::Verified, 1),
            (Phase::RollbackIntent, 1),
            (Phase::RolledBack, 2),
        ] {
            let b = binding();
            let mut d = FakeDisk::new(&b);
            d.fail_phase = Some(phase);
            let verification = if matches!(phase, Phase::RollbackIntent | Phase::RolledBack) {
                Verification::Rejected
            } else {
                Verification::Verified
            };
            assert_eq!(
                apply(&b, &mut d, || verification),
                Outcome::NeedsInspection,
                "{phase:?}"
            );
            assert_eq!(d.swaps, swaps, "{phase:?}");
        }
    }
    #[test]
    fn failed_first_swap_is_settled_without_second_swap() {
        for changed in [false, true] {
            let b = binding();
            let mut d = FakeDisk::new(&b);
            d.swap_failure = Some((1, changed));
            assert_eq!(
                apply(&b, &mut d, || panic!("unconfirmed swap")),
                if changed {
                    Outcome::NeedsInspection
                } else {
                    Outcome::NotInstalled
                }
            );
            assert_eq!(d.swaps, 1);
            assert_eq!(
                d.layout,
                if changed {
                    b.swapped()
                } else {
                    b.original.clone()
                }
            );
        }
    }
    #[test]
    fn rollback_error_preserves_uncertainty_even_after_observed_restoration() {
        for changed in [false, true] {
            let b = binding();
            let mut d = FakeDisk::new(&b);
            d.swap_failure = Some((2, changed));
            assert_eq!(
                apply(&b, &mut d, || Verification::Rejected),
                Outcome::NeedsInspection
            );
            assert_eq!(d.swaps, 2);
            assert_eq!(
                inspect(&b, &mut d),
                if changed {
                    Inspection::Original
                } else {
                    Inspection::SwappedWithBackup
                }
            );
            assert!(!d.phases.contains(&Phase::RolledBack));
        }
    }
    #[test]
    fn namespace_change_after_durable_intent_preserves_foreign_object() {
        for phase in [
            Phase::Intent,
            Phase::Swapped,
            Phase::Verified,
            Phase::RollbackIntent,
            Phase::RolledBack,
        ] {
            let b = binding();
            let mut d = FakeDisk::new(&b);
            d.mutate_after_record = Some(phase);
            assert_eq!(
                apply(&b, &mut d, || if phase == Phase::Verified {
                    Verification::Verified
                } else {
                    Verification::Rejected
                }),
                Outcome::NeedsInspection
            );
            assert_eq!(d.layout.current.inode, 99);
            assert_eq!(
                d.swaps,
                if phase == Phase::Intent {
                    0
                } else if phase == Phase::RolledBack {
                    2
                } else {
                    1
                }
            );
        }
    }
    #[test]
    fn untrusted_or_unavailable_initial_snapshot_has_no_writes() {
        let b = binding();
        let mut d = FakeDisk::new(&b);
        d.layout.current.inode = 99;
        assert_eq!(
            apply(&b, &mut d, || panic!("no mutation")),
            Outcome::RejectedBeforeMutation
        );
        d.unavailable = true;
        assert_eq!(
            apply(&b, &mut d, || panic!("no mutation")),
            Outcome::RejectedBeforeMutation
        );
        assert_eq!(d.swaps, 0);
        assert!(d.phases.is_empty());
    }
    #[test]
    fn binding_rejects_other_volumes_users_aliases_and_non_upgrade() {
        let b = binding();
        let check = |layout: Layout, from: &str, to: &str, hash: &str, uid| {
            Binding::new(
                b.transaction_id.clone(),
                from.into(),
                to.into(),
                hash.into(),
                layout,
                uid,
            )
            .is_err()
        };
        assert!(check(
            b.original.clone(),
            "0.1.0",
            "0.1.0",
            &b.archive_sha256,
            501
        ));
        assert!(check(
            b.original.clone(),
            "0.2.0",
            "0.1.0",
            &b.archive_sha256,
            501
        ));
        assert!(check(b.original.clone(), "0.1.0", "0.1.1", "ABC", 501));
        assert!(check(
            b.original.clone(),
            "0.1.0",
            "0.1.1",
            &b.archive_sha256,
            0
        ));
        for mutate in [0, 1, 2, 3, 4, 5] {
            let mut l = b.original.clone();
            match mutate {
                0 => l.candidate.device = 2,
                1 => l.candidate.uid = 502,
                2 => l.workspace.permissions = 0o755,
                3 => l.parent.permissions = 0o777,
                4 => l.candidate.inode = l.current.inode,
                _ => l.current.permissions = 0o4755,
            };
            assert!(check(l, "0.1.0", "0.1.1", &b.archive_sha256, 501));
        }
    }
}
