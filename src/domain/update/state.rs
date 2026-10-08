//! Where an update is in its lifecycle, and how that reads on screen.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Downloading {
        version: String,
        received: u64,
        total: Option<u64>,
    },
    Installing {
        version: String,
    },
    Restarting {
        version: String,
    },
    Failed {
        message: String,
    },
}

impl UpdateState {
    pub fn from_check_error() -> Self {
        Self::UpToDate
    }

    pub fn is_settled(&self) -> bool {
        matches!(self, Self::UpToDate)
    }

    pub fn needs_acknowledgement(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }

    pub fn is_working(&self) -> bool {
        matches!(
            self,
            Self::Checking | Self::Downloading { .. } | Self::Installing { .. }
        )
    }

    pub fn status_line(&self) -> String {
        match self {
            Self::Idle => String::new(),
            Self::Checking => "Checking for updates…".to_owned(),
            Self::UpToDate => "Up to date".to_owned(),
            Self::Downloading { version, .. } => format!("Downloading v{version}…"),
            Self::Installing { version } => format!("Installing v{version}…"),
            Self::Restarting { version } => format!("Restarting into v{version}…"),
            Self::Failed { message } => message.clone(),
        }
    }

    pub fn progress(&self) -> Option<f32> {
        match self {
            Self::Downloading {
                received,
                total: Some(total),
                ..
            } if *total > 0 => Some((*received as f32 / *total as f32).clamp(0.0, 1.0)),
            _ => None,
        }
    }

    pub fn byte_readout(&self) -> Option<String> {
        match self {
            Self::Downloading {
                received,
                total: Some(total),
                ..
            } => Some(format!("{} / {}", megabytes(*received), megabytes(*total))),
            Self::Downloading {
                received,
                total: None,
                ..
            } => Some(megabytes(*received)),
            _ => None,
        }
    }
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::update::UpdateError;

    #[test]
    fn a_failed_check_is_not_shown_to_the_user() {
        let state = UpdateState::from_check_error();
        assert_eq!(state, UpdateState::UpToDate);
        assert!(state.is_settled(), "the splash moves on");
        assert!(!state.needs_acknowledgement(), "and asks for nothing");
    }

    #[test]
    fn a_failed_download_waits_for_the_user() {
        let state = UpdateState::Failed {
            message: UpdateError::Download("connection reset".into()).to_string(),
        };
        assert!(state.needs_acknowledgement());
        assert!(!state.is_settled(), "it must not advance on its own");
        assert!(state.status_line().contains("connection reset"));
    }

    #[test]
    fn only_the_working_states_report_as_working() {
        assert!(UpdateState::Checking.is_working());
        assert!(
            UpdateState::Downloading {
                version: "1.0.0".into(),
                received: 1,
                total: Some(2),
            }
            .is_working()
        );
        assert!(
            UpdateState::Installing {
                version: "1.0.0".into()
            }
            .is_working()
        );

        assert!(!UpdateState::Idle.is_working());
        assert!(!UpdateState::UpToDate.is_working());
        assert!(
            !UpdateState::Failed {
                message: "x".into()
            }
            .is_working()
        );
    }

    #[test]
    fn progress_is_the_fraction_downloaded() {
        let half = UpdateState::Downloading {
            version: "1.0.0".into(),
            received: 50,
            total: Some(100),
        };
        assert_eq!(half.progress(), Some(0.5));
        assert_eq!(half.byte_readout().unwrap(), "0.0 MB / 0.0 MB");
    }

    #[test]
    fn a_zero_total_does_not_divide_by_zero() {
        let state = UpdateState::Downloading {
            version: "1.0.0".into(),
            received: 0,
            total: Some(0),
        };
        assert_eq!(state.progress(), None, "indeterminate, not NaN");
    }

    #[test]
    fn overshooting_the_reported_size_clamps_to_full() {
        let state = UpdateState::Downloading {
            version: "1.0.0".into(),
            received: 300,
            total: Some(100),
        };
        assert_eq!(state.progress(), Some(1.0));
    }

    #[test]
    fn an_unknown_total_leaves_the_bar_indeterminate_but_still_counts_bytes() {
        let state = UpdateState::Downloading {
            version: "1.0.0".into(),
            received: 5 * 1024 * 1024,
            total: None,
        };
        assert_eq!(state.progress(), None);
        assert_eq!(state.byte_readout().unwrap(), "5.0 MB");
    }

    #[test]
    fn states_without_a_download_have_no_bar_or_readout() {
        for state in [
            UpdateState::Idle,
            UpdateState::Checking,
            UpdateState::UpToDate,
            UpdateState::Installing {
                version: "1.0.0".into(),
            },
        ] {
            assert_eq!(state.progress(), None);
            assert_eq!(state.byte_readout(), None);
        }
    }

    #[test]
    fn status_lines_name_the_version_being_installed() {
        assert_eq!(UpdateState::Checking.status_line(), "Checking for updates…");
        assert_eq!(
            UpdateState::Downloading {
                version: "1.2.0".into(),
                received: 0,
                total: None,
            }
            .status_line(),
            "Downloading v1.2.0…"
        );
        assert_eq!(
            UpdateState::Installing {
                version: "1.2.0".into()
            }
            .status_line(),
            "Installing v1.2.0…"
        );
    }
}
