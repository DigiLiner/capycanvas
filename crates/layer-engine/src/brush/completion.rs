use layer_core::Point;
use layer_render::Dab;
use std::{collections::VecDeque, sync::Arc};

#[derive(Clone, Copy, Debug)]
struct Vertex { dab: Dab, anchor: Point, distance: f64 }

#[derive(Clone, Debug, Default)]
pub(super) struct Path {
    vertices: Arc<VecDeque<Vertex>>,
    next: usize,
    started: bool,
}

fn vector(a: Point, b: Point) -> [f32; 2] { [b.x-a.x, b.y-a.y] }
fn length(v: [f32; 2]) -> f32 { v[0].hypot(v[1]) }
fn unit(v: [f32; 2]) -> [f32; 2] { let n=length(v).max(1e-12); v.map(|x|x/n) }
fn smooth(x: f32) -> f32 { let t=x.clamp(0.,1.); t*t*(3.-2.*t) }

impl Path {
    pub fn append(&mut self, dab: Dab) -> Vec<Dab> {
        let mut output=Vec::new();
        if self.vertices.is_empty() {
            Arc::make_mut(&mut self.vertices).push_back(Vertex { dab, anchor: dab.center, distance: 0. });
            self.next=1;
            return output;
        }
        let last=*self.vertices.back().unwrap();
        let distance=last.distance+f64::from(length(vector(last.dab.center,dab.center)));
        if distance<=last.distance {return output;}
        if !self.started {
            output.push(last.dab.completion([0.;2],unit(vector(last.dab.center,dab.center)),1.));
            self.started=true;
        }
        let radius=dab.radii[0].min(dab.radii[1]).max(0.005);
        let merge=self.vertices.len()>self.next && self.vertices.len()>1
            && length(vector(last.anchor,dab.center))<(radius*0.05).max(0.25);
        let vertices=Arc::make_mut(&mut self.vertices);
        if merge {
            let last=vertices.back_mut().unwrap();
            last.dab=dab;
            last.distance=distance;
        } else {vertices.push_back(Vertex {dab,anchor:dab.center,distance});}
        self.resolve(false,&mut output);
        output
    }

    fn point_at(&self, distance: f64) -> Point {
        let mut prior=self.vertices.front().unwrap();
        for next in self.vertices.iter().skip(1) {
            if next.distance>=distance {
                let t=((distance-prior.distance)/(next.distance-prior.distance).max(1e-12)).clamp(0.,1.) as f32;
                return Point {x:prior.dab.center.x+t*(next.dab.center.x-prior.dab.center.x),
                    y:prior.dab.center.y+t*(next.dab.center.y-prior.dab.center.y)};
            }
            prior=next;
        }
        prior.dab.center
    }

    fn corner(&self, first: usize, mut last: usize) -> usize {
        let start=self.vertices[first].dab;
        let tolerance=(start.radii[0].min(start.radii[1])*0.025).max(0.01);
        loop {
            let chord=vector(start.center,self.vertices[last].dab.center);
            let norm=(chord[0]*chord[0]+chord[1]*chord[1]).max(1e-12);
            let mut maximum=tolerance*tolerance;
            let mut split=last;
            for i in first+1..last {
                let point=vector(start.center,self.vertices[i].dab.center);
                let t=((point[0]*chord[0]+point[1]*chord[1])/norm).clamp(0.,1.);
                let error=(point[0]-t*chord[0]).powi(2)+(point[1]-t*chord[1]).powi(2);
                if error>maximum {maximum=error;split=i;}
            }
            if split==last {return last;}
            last=split;
        }
    }

    fn resolve(&mut self, finish: bool, output: &mut Vec<Dab>) {
        while self.next+1<self.vertices.len() {
            let first=self.next-1;
            let i=self.corner(first,self.vertices.len()-1);
            if i+1==self.vertices.len() {
                if !finish && self.vertices.len()>=256 {self.next=i;}
                break;
            }
            let vertex=self.vertices[i];
            let radius=f64::from(vertex.dab.radii[0].min(vertex.dab.radii[1]).max(0.005));
            let end=self.vertices.back().unwrap().distance;
            if !finish && end-vertex.distance<radius && self.vertices.len()<256 {break;}
            let before=vector(self.point_at(vertex.distance-radius),vertex.dab.center);
            let after=vector(vertex.dab.center,self.point_at(vertex.distance+radius));
            let a=length(before);let b=length(after);
            if a>1e-6 && b>1e-6 {
                let cosine=((before[0]*after[0]+before[1]*after[1])/(a*b)).clamp(-1.,1.);
                let angle=cosine.acos().to_degrees();
                let reach=(f64::from(a)/(vertex.distance-self.vertices.front().unwrap().distance).min(radius).max(1e-12))
                    .min(f64::from(b)/(end-vertex.distance).min(radius).max(1e-12)) as f32;
                let incoming=unit(vector(vertex.dab.center,self.vertices[first].dab.center));
                let outgoing=unit(vector(vertex.dab.center,self.vertices[self.corner(i,self.vertices.len()-1)].dab.center));
                let dot=incoming[0]*outgoing[0]+incoming[1]*outgoing[1];
                let weight=smooth((angle-10.)/20.)*smooth((reach-0.3)/0.3);
                if dot > -0.999999 && weight>0. {
                    output.push(vertex.dab.completion(incoming,outgoing,weight));
                }
            }
            self.next=i+1;
        }
        if !finish && self.next>1 {
            let next=self.vertices[self.next.min(self.vertices.len()-1)];
            let radius=f64::from(next.dab.radii[0].min(next.dab.radii[1]).max(0.005));
            let vertices=Arc::make_mut(&mut self.vertices);
            while self.next>1 && (vertices[1].distance<next.distance-radius || vertices.len()>=256) {
                vertices.pop_front();self.next-=1;
            }
        }
    }

    pub fn finish(&mut self) -> Vec<Dab> {
        let mut output=Vec::new();
        if self.started && self.vertices.len()>1 {
            self.resolve(true,&mut output);
            let end=self.vertices.back().unwrap().dab;
            let previous=self.vertices[self.next.saturating_sub(1).min(self.vertices.len()-2)].dab;
            output.push(end.completion(unit(vector(end.center,previous.center)),[0.;2],1.));
        }
        *self=Self::default();
        output
    }
}
