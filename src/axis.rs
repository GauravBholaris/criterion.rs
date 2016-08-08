use std::collections::TreeMap;
use std::str::MaybeOwned;

use {Data, Script, grid};
use display::Display;

#[deriving(Clone)]
pub struct Properties {
    grids: TreeMap<grid::Grid, grid::Properties>,
    hidden: bool,
    label: Option<MaybeOwned<'static>>,
    logarithmic: bool,
    range: Option<(f64, f64)>,
    tics: Option<String>,
}

impl Properties {
    // NB I dislike the visibility rules within the same crate
    #[doc(hidden)]
    pub fn _new() -> Properties {
        Properties {
            grids: TreeMap::new(),
            hidden: false,
            label: None,
            logarithmic: false,
            range: None,
            tics: None,
        }
    }

    /// Autoscales the range of the axis to show all the plot elements
    ///
    /// **Note** All axes are auto-scaled by default
    pub fn autoscale(&mut self) -> &mut Properties {
        self.range = None;
        self
    }

    /// Configures the gridlines
    pub fn grid(
        &mut self,
        which: grid::Grid,
        configure: <'a> |&'a mut grid::Properties| -> &'a mut grid::Properties,
    ) -> &mut Properties {
        if self.grids.contains_key(&which) {
            configure(self.grids.get_mut(&which).unwrap());
        } else {
            let mut properties = grid::Properties::_new();
            configure(&mut properties);
            self.grids.insert(which, properties);
        }
        self
    }

    /// Hides the axis
    ///
    /// **Note** The `TopX` and `RightY` axes are hidden by default
    pub fn hide(&mut self) -> &mut Properties {
        self.hidden = true;
        self
    }

    /// Attaches a label to the axis
    pub fn label<S>(&mut self, label: S) -> &mut Properties where S: IntoMaybeOwned<'static> {
        self.label = Some(label.into_maybe_owned());
        self
    }

    /// Changes the range of the axis that will be shown
    pub fn range(&mut self, low: f64, high: f64) -> &mut Properties {
        self.hidden = false;
        self.range = Some((low, high));
        self
    }

    /// Sets the scale of the axis
    ///
    /// **Note** All axes use a linear scale by default
    pub fn scale(&mut self, scale: Scale) -> &mut Properties {
        self.hidden = false;
        match scale {
            Linear => self.logarithmic = false,
            Logarithmic => self.logarithmic = true,
        }
        self
    }

    /// Attaches labels to the tics of an axis
    // TODO Configuration: rotation, font, etc
    pub fn tics<A, S, P, L>(&mut self, pos: P, labels: L) -> &mut Properties where
        A: Data, P: Iterator<A>, S: Str, L: Iterator<S>
    {
        let pairs = pos.zip(labels).map(|(pos, label)| {
            format!("'{}' {}", label.as_slice(), pos.f64())
        }).collect::<Vec<_>>();

        if pairs.len() == 0 {
            self.tics = None
        } else {
            self.tics = Some(pairs.connect(", "));
        }

        self
    }

    /// Makes the axis visible
    ///
    /// **Note** The `BottomX` and `LeftY` axes are visible by default
    pub fn show(&mut self) -> &mut Properties {
        self.hidden = false;
        self
    }
}

impl<'a, 'b> Script for (&'a Axis, &'b Properties) {
    fn script(&self) -> String {
        let &(axis, properties) = self;
        let axis_ = axis.display();

        let mut script = if properties.hidden {
            return format!("unset {}tics\n", axis_);
        } else {
            format!("set {}tics nomirror ", axis_)
        };

        if let Some(ref tics) = properties.tics {
            script.push_str(format!("({})", tics)[])
        }

        script.push('\n');

        if let Some(ref label) = properties.label {
            script.push_str(format!("set {}label '{}'\n", axis_, label)[])
        }

        if let Some((low, high)) = properties.range {
            script.push_str(format!("set {}range [{}:{}]\n", axis_, low, high)[])
        }

        if properties.logarithmic {
            script.push_str(format!("set logscale {}\n", axis_)[]);
        }

        for (grid, properties) in properties.grids.iter() {
            script.push_str((axis, grid, properties).script()[]);
        }

        script
    }
}

#[deriving(Clone, Eq, Ord, PartialEq, PartialOrd)]
pub enum Axis {
    BottomX,
    LeftY,
    RightY,
    TopX,
}

#[doc(hidden)]
impl Display<&'static str> for Axis {
    fn display(&self) -> &'static str {
        match *self {
            BottomX => "x",
            LeftY => "y",
            RightY => "y2",
            TopX => "x2",
        }
    }
}

pub enum Scale {
    Linear,
    Logarithmic,
}
