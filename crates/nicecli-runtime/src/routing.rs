#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RoutingStrategy {
    #[default]
    RoundRobin,
    FillFirst,
}

impl RoutingStrategy {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "round-robin" | "roundrobin" | "rr" => Some(Self::RoundRobin),
            "fill-first" | "fillfirst" | "ff" => Some(Self::FillFirst),
            _ => None,
        }
    }

    pub fn from_config_value(value: Option<&str>) -> Self {
        value.and_then(Self::parse).unwrap_or_default()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::RoundRobin => "round-robin",
            Self::FillFirst => "fill-first",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RoutingStrategy;

    #[test]
    fn routing_strategy_parses_aliases() {
        assert_eq!(
            RoutingStrategy::parse("round-robin"),
            Some(RoutingStrategy::RoundRobin)
        );
        assert_eq!(
            RoutingStrategy::parse("roundrobin"),
            Some(RoutingStrategy::RoundRobin)
        );
        assert_eq!(
            RoutingStrategy::parse("rr"),
            Some(RoutingStrategy::RoundRobin)
        );
        assert_eq!(
            RoutingStrategy::parse("fill-first"),
            Some(RoutingStrategy::FillFirst)
        );
        assert_eq!(
            RoutingStrategy::parse("fillfirst"),
            Some(RoutingStrategy::FillFirst)
        );
        assert_eq!(
            RoutingStrategy::parse("ff"),
            Some(RoutingStrategy::FillFirst)
        );
    }

    #[test]
    fn routing_strategy_defaults_to_round_robin() {
        assert_eq!(
            RoutingStrategy::from_config_value(None),
            RoutingStrategy::RoundRobin
        );
        assert_eq!(
            RoutingStrategy::from_config_value(Some("")),
            RoutingStrategy::RoundRobin
        );
    }

    #[test]
    fn routing_strategy_rejects_unknown_values() {
        assert_eq!(RoutingStrategy::parse("random"), None);
    }

    #[test]
    fn routing_strategy_serializes_to_stable_strings() {
        assert_eq!(RoutingStrategy::RoundRobin.as_str(), "round-robin");
        assert_eq!(RoutingStrategy::FillFirst.as_str(), "fill-first");
    }
}
