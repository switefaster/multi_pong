use amethyst::core::ecs::shred::SystemData;
use amethyst::core::ecs::{
    Component, DenseVecStorage, DispatcherBuilder, Join, ReadStorage, World, WorldExt,
};
use amethyst::renderer::bundle::{RenderOrder, RenderPlan, Target};
use amethyst::renderer::pipeline::{PipelineDescBuilder, PipelinesBuilder};
use amethyst::renderer::rendy::command::{QueueId, RenderPassEncoder};
use amethyst::renderer::rendy::graph::render::{PrepareResult, RenderGroup};
use amethyst::renderer::rendy::graph::{GraphContext, NodeBuffer, NodeImage};
use amethyst::renderer::rendy::hal::pass::Subpass;
use amethyst::renderer::rendy::hal::pso::InputAssemblerDesc;
use amethyst::renderer::rendy::hal::*;
use amethyst::renderer::rendy::mesh::{AsVertex, VertexFormat};
use amethyst::renderer::rendy::shader::{
    PathBufShaderInfo, Shader, ShaderKind, SourceLanguage, SpirvShader,
};
use amethyst::renderer::rendy::*;
use amethyst::renderer::submodules::DynamicVertexBuffer;
use amethyst::renderer::{util, ChangeDetection, RenderPlugin};
use amethyst::renderer::{Backend, Factory, Format, RenderGroupDesc};
use derivative::Derivative;
use glsl_layout::*;
use std::path::PathBuf;

lazy_static::lazy_static! {
    static ref VERTEX: SpirvShader = PathBufShaderInfo::new(
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/shader/inv_color.vert")),
        ShaderKind::Vertex,
        SourceLanguage::GLSL,
        "main",
    ).precompile().unwrap();

    static ref FRAGMENT: SpirvShader = PathBufShaderInfo::new(
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/shader/inv_color.frag")),
        ShaderKind::Fragment,
        SourceLanguage::GLSL,
        "main",
    ).precompile().unwrap();
}

#[derive(Clone, Debug, PartialEq, Derivative)]
#[derivative(Default(bound = ""))]
pub struct DrawInvertColorDesc;

impl DrawInvertColorDesc {
    pub fn new() -> Self {
        Default::default()
    }
}

impl<B: Backend> RenderGroupDesc<B, World> for DrawInvertColorDesc {
    fn build<'a>(
        self,
        _ctx: &GraphContext<B>,
        factory: &mut Factory<B>,
        _queue: QueueId,
        _aux: &World,
        framebuffer_width: u32,
        framebuffer_height: u32,
        subpass: Subpass<'_, B>,
        _buffers: Vec<NodeBuffer>,
        _images: Vec<NodeImage>,
    ) -> Result<Box<dyn RenderGroup<B, World>>, failure::Error> {
        let vertex = DynamicVertexBuffer::new();

        let (pipeline, pipeline_layout) = build_invert_color_pipeline(
            factory,
            subpass,
            framebuffer_width,
            framebuffer_height,
            vec![],
        )?;

        Ok(Box::new(DrawInvertColor::<B> {
            pipeline,
            pipeline_layout,
            vertex,
            vertex_count: 0,
            change: Default::default(),
        }))
    }
}

#[derive(Debug)]
pub struct DrawInvertColor<B: Backend> {
    pipeline: B::GraphicsPipeline,
    pipeline_layout: B::PipelineLayout,
    vertex: DynamicVertexBuffer<B, InvertColorArgs>,
    vertex_count: usize,
    change: ChangeDetection,
}

impl<B: Backend> RenderGroup<B, World> for DrawInvertColor<B> {
    fn prepare(
        &mut self,
        factory: &Factory<B>,
        _queue: QueueId,
        index: usize,
        _subpass: Subpass<'_, B>,
        aux: &World,
    ) -> PrepareResult {
        let (triangles,) = <(ReadStorage<'_, InvColorTriangle>,)>::fetch(aux);
        let old_vertex_count = self.vertex_count;
        self.vertex_count = (&triangles).join().count() * 3;
        let changed = old_vertex_count != self.vertex_count;
        let vertex_data_iter = (&triangles).join().flat_map(|triangle| triangle.get_args());
        self.vertex.write(
            factory,
            index,
            self.vertex_count as u64,
            Some(vertex_data_iter.collect::<Box<[InvertColorArgs]>>()),
        );
        self.change.prepare_result(index, changed)
    }

    fn draw_inline(
        &mut self,
        mut encoder: RenderPassEncoder<'_, B>,
        index: usize,
        _subpass: Subpass<'_, B>,
        _aux: &World,
    ) {
        if self.vertex_count == 0 {
            return;
        }
        encoder.bind_graphics_pipeline(&self.pipeline);
        self.vertex.bind(index, 0, 0, &mut encoder);
        unsafe {
            encoder.draw(0..self.vertex_count as u32, 0..1);
        }
    }

    fn dispose(self: Box<Self>, factory: &mut Factory<B>, _aux: &World) {
        unsafe {
            factory.device().destroy_graphics_pipeline(self.pipeline);
            factory
                .device()
                .destroy_pipeline_layout(self.pipeline_layout);
        }
    }
}

fn build_invert_color_pipeline<B: Backend>(
    factory: &Factory<B>,
    subpass: hal::pass::Subpass<'_, B>,
    framebuffer_width: u32,
    framebuffer_height: u32,
    layouts: Vec<&B::DescriptorSetLayout>,
) -> Result<(B::GraphicsPipeline, B::PipelineLayout), failure::Error> {
    let pipeline_layout = unsafe {
        factory
            .device()
            .create_pipeline_layout(layouts, None as Option<(_, _)>)
    }?;

    let shader_vertex = unsafe { VERTEX.module(factory).unwrap() };
    let shader_fragment = unsafe { FRAGMENT.module(factory).unwrap() };

    let pipes = PipelinesBuilder::new()
        .with_pipeline(
            PipelineDescBuilder::new()
                .with_vertex_desc(&[(InvertColorArgs::vertex(), pso::VertexInputRate::Vertex)])
                .with_input_assembler(InputAssemblerDesc::new(Primitive::TriangleList))
                .with_shaders(util::simple_shader_set(
                    &shader_vertex,
                    Some(&shader_fragment),
                ))
                .with_layout(&pipeline_layout)
                .with_subpass(subpass)
                .with_framebuffer_size(framebuffer_width, framebuffer_height)
                .with_blend_targets(vec![pso::ColorBlendDesc {
                    mask: pso::ColorMask::ALL,
                    blend: Some(pso::BlendState {
                        alpha: pso::BlendOp::Add {
                            src: pso::Factor::Zero,
                            dst: pso::Factor::One,
                        },
                        color: pso::BlendOp::Sub {
                            src: pso::Factor::One,
                            dst: pso::Factor::One,
                        },
                    }),
                }]),
        )
        .build(factory, None);

    unsafe {
        factory.destroy_shader_module(shader_vertex);
        factory.destroy_shader_module(shader_fragment);
    }

    match pipes {
        Err(e) => {
            unsafe {
                factory.device().destroy_pipeline_layout(pipeline_layout);
            }
            Err(e)
        }
        Ok(mut pipes) => Ok((pipes.remove(0), pipeline_layout)),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, AsStd140)]
#[repr(C, align(4))]
pub struct InvertColorArgs {
    pub pos: vec2,
}

impl AsVertex for InvertColorArgs {
    fn vertex() -> VertexFormat {
        VertexFormat::new(((Format::Rg32Sfloat, "pos"),))
    }
}

#[derive(Default, Debug)]
pub struct RenderInvColor {}

impl<B: Backend> RenderPlugin<B> for RenderInvColor {
    fn on_build<'a, 'b>(
        &mut self,
        world: &mut World,
        _builder: &mut DispatcherBuilder<'a, 'b>,
    ) -> Result<(), amethyst::Error> {
        world.register::<InvColorTriangle>();
        Ok(())
    }

    fn on_plan(
        &mut self,
        plan: &mut RenderPlan<B>,
        _factory: &mut Factory<B>,
        _world: &World,
    ) -> Result<(), amethyst::Error> {
        plan.extend_target(Target::Main, |ctx| {
            ctx.add(
                RenderOrder::LinearPostEffects,
                DrawInvertColorDesc::new().builder(),
            )?;
            Ok(())
        });
        Ok(())
    }
}

#[derive(Default, Debug)]
pub struct InvColorTriangle {
    pub points: [[f32; 2]; 3],
}

impl Component for InvColorTriangle {
    type Storage = DenseVecStorage<Self>;
}

impl InvColorTriangle {
    pub fn get_args(&self) -> Vec<InvertColorArgs> {
        let mut vec = Vec::new();
        vec.extend((0..3).map(|i| InvertColorArgs {
            pos: self.points[i].into(),
        }));
        vec
    }
}
