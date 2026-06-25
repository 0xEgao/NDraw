//! Stateful validation and reconstruction of one turn's shared canvas.

use std::collections::HashSet;

use ndraw_proto::{
    CanvasSnapshot, DrawOp, Point, Stroke, StrokeId, Validate,
    limit::{MAX_POINTS_PER_STROKE, MAX_STROKES_PER_SNAPSHOT},
};

use crate::error::CanvasError;

/// Total point budget retained for one current-turn drawing.
pub const MAX_CANVAS_POINTS: usize = 50_000;

/// Stroke currently receiving point batches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveStroke {
    /// Stroke identity.
    pub stroke_id: StrokeId,
    /// RGB color encoded as `0x00RRGGBB`.
    pub color: u32,
    /// Brush width.
    pub width: u8,
    /// Accumulated points, including the starting point.
    pub points: Vec<Point>,
    /// Next accepted point-batch or end sequence.
    pub next_sequence: u16,
}

/// Authoritative current-turn drawing state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanvasState {
    background_color: u32,
    completed: Vec<Stroke>,
    active: Option<ActiveStroke>,
    used_ids: HashSet<StrokeId>,
    total_points: usize,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            background_color: 0x00ff_ffff,
            completed: Vec::new(),
            active: None,
            used_ids: HashSet::new(),
            total_points: 0,
        }
    }
}

impl CanvasState {
    /// Applies one already authenticated drawer operation.
    pub fn apply(&mut self, operation: &DrawOp) -> Result<(), CanvasError> {
        operation
            .validate()
            .map_err(CanvasError::InvalidOperation)?;

        match operation {
            DrawOp::Begin {
                stroke_id,
                color,
                width,
                start,
            } => self.begin(*stroke_id, *color, *width, *start),
            DrawOp::Points {
                stroke_id,
                sequence,
                points,
            } => self.append(*stroke_id, *sequence, points),
            DrawOp::End {
                stroke_id,
                sequence,
            } => self.end(*stroke_id, *sequence),
            DrawOp::Undo => self.undo(),
            DrawOp::Clear => {
                self.clear();
                Ok(())
            }
            DrawOp::Fill { color } => self.fill(*color),
        }
    }

    fn begin(
        &mut self,
        stroke_id: StrokeId,
        color: u32,
        width: u8,
        start: Point,
    ) -> Result<(), CanvasError> {
        if self.active.is_some() {
            return Err(CanvasError::StrokeAlreadyActive);
        }
        if self.used_ids.contains(&stroke_id) {
            return Err(CanvasError::DuplicateStroke);
        }
        if self.completed.len() >= MAX_STROKES_PER_SNAPSHOT {
            return Err(CanvasError::StrokeBudgetExceeded);
        }
        if self.total_points >= MAX_CANVAS_POINTS {
            return Err(CanvasError::PointBudgetExceeded);
        }

        self.used_ids.insert(stroke_id);
        self.total_points += 1;
        self.active = Some(ActiveStroke {
            stroke_id,
            color,
            width,
            points: vec![start],
            next_sequence: 0,
        });
        Ok(())
    }

    fn append(
        &mut self,
        stroke_id: StrokeId,
        sequence: u16,
        points: &[Point],
    ) -> Result<(), CanvasError> {
        let active = self.active.as_mut().ok_or(CanvasError::NoActiveStroke)?;
        if active.stroke_id != stroke_id {
            return Err(CanvasError::WrongStroke);
        }
        if active.next_sequence != sequence {
            return Err(CanvasError::WrongSequence {
                expected: active.next_sequence,
                received: sequence,
            });
        }
        let new_stroke_points = active.points.len().saturating_add(points.len());
        if new_stroke_points > MAX_POINTS_PER_STROKE {
            return Err(CanvasError::PointBudgetExceeded);
        }
        if self.total_points.saturating_add(points.len()) > MAX_CANVAS_POINTS {
            return Err(CanvasError::PointBudgetExceeded);
        }
        let next_sequence = active
            .next_sequence
            .checked_add(1)
            .ok_or(CanvasError::SequenceExhausted)?;

        active.points.extend_from_slice(points);
        active.next_sequence = next_sequence;
        self.total_points += points.len();
        Ok(())
    }

    fn end(&mut self, stroke_id: StrokeId, sequence: u16) -> Result<(), CanvasError> {
        let active = self.active.as_ref().ok_or(CanvasError::NoActiveStroke)?;
        if active.stroke_id != stroke_id {
            return Err(CanvasError::WrongStroke);
        }
        if active.next_sequence != sequence {
            return Err(CanvasError::WrongSequence {
                expected: active.next_sequence,
                received: sequence,
            });
        }

        let active = self.active.take().ok_or(CanvasError::NoActiveStroke)?;
        self.completed.push(Stroke {
            stroke_id: active.stroke_id,
            color: active.color,
            width: active.width,
            points: active.points,
        });
        Ok(())
    }

    fn undo(&mut self) -> Result<(), CanvasError> {
        if self.active.is_some() {
            return Err(CanvasError::StrokeAlreadyActive);
        }
        if let Some(stroke) = self.completed.pop() {
            self.total_points = self.total_points.saturating_sub(stroke.points.len());
        }
        Ok(())
    }

    fn fill(&mut self, color: u32) -> Result<(), CanvasError> {
        if self.active.is_some() {
            return Err(CanvasError::StrokeAlreadyActive);
        }
        self.background_color = color;
        Ok(())
    }

    /// Finalizes an active stroke, used when a drawer disconnects mid-stroke.
    pub fn finalize_active(&mut self) {
        if let Some(active) = self.active.take() {
            self.completed.push(Stroke {
                stroke_id: active.stroke_id,
                color: active.color,
                width: active.width,
                points: active.points,
            });
        }
    }

    /// Removes all drawing state while retaining used IDs for this turn.
    pub fn clear(&mut self) {
        self.background_color = 0x00ff_ffff;
        self.completed.clear();
        self.active = None;
        self.total_points = 0;
    }

    /// Returns a reconnect-safe rendering snapshot.
    #[must_use]
    pub fn snapshot(&self) -> CanvasSnapshot {
        let mut strokes = self.completed.clone();
        if let Some(active) = &self.active {
            strokes.push(Stroke {
                stroke_id: active.stroke_id,
                color: active.color,
                width: active.width,
                points: active.points.clone(),
            });
        }
        CanvasSnapshot {
            background_color: self.background_color,
            strokes,
        }
    }

    /// Number of retained points across completed and active strokes.
    #[must_use]
    pub const fn total_points(&self) -> usize {
        self.total_points
    }
}
