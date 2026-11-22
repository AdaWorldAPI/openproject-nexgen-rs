# frozen_string_literal: true

# Records what OpenProject's own WorkPackages::CreateService does, not only what
# its contract answers: the service result, the ordered callback journal of the
# operation under test (setup is not journaled), the transaction depth of every
# journaled callback, the row-count deltas of the call, and whether the persisted
# record would pass ActiveRecord validation on its own. The last fact makes the
# `save(validate: false)` boundary in create_service.rb observable.
#
# Callbacks run their real bodies; the journal only observes. The operation is
# NOT wrapped in a rollback, so after_commit callbacks fire. Run it against a
# disposable test database: it commits rows.
#
#   cd <openproject checkout> && RAILS_ENV=test bin/rails runner <this file>
#
# Output path: ENV["OP_SERVICE_TRACE_OUT"], default work-package-service-trace.json
# next to this file. No ids or timestamps are recorded.

require "json"
require "factory_bot"
require "test_prof"
require "test_prof/factory_default"
TestProf::FactoryDefault.init

STUBS = [
  "ActionMailer::Base.delivery_method = :test (no mail is delivered)",
  "ActiveJob::Base.queue_adapter = :test (no job is performed; enqueued job classes are recorded)",
  "Rails.cache replaced by ActiveSupport::Cache::NullStore",
  "RequestStore cleared between cases"
].freeze

ActionMailer::Base.delivery_method = :test
ActionMailer::Base.perform_deliveries = false
ActiveJob::Base.queue_adapter = :test
Rails.cache = ActiveSupport::Cache::NullStore.new

root = Rails.root
FactoryBot.definition_file_paths = [root.join("spec/factories").to_s] +
                                   Dir[root.join("modules/*/spec/factories").to_s]
FactoryBot.find_definitions
include FactoryBot::Syntax::Methods # rubocop:disable Style/MixinUsage

# --- callback journal (observes; every body still runs) ----------------------

$journal = nil
$chain_stack = []

ActiveSupport::Callbacks.prepend(Module.new do
  def run_callbacks(kind, type = nil, &)
    $chain_stack.push(kind)
    super
  ensure
    $chain_stack.pop
  end
end)

def tx_depth
  ActiveRecord::Base.connection.open_transactions
end

def journal(record, chain, slot, name)
  $journal&.push([record.class.name, "#{slot}_#{chain}", name, tx_depth])
end

def rel(path)
  path.to_s.sub("#{Rails.root}/", "")
end

instrumented = Set.new
Rails.application.eager_load!
ActiveRecord::Base.descendants.each do |klass|
  next if klass.abstract_class? || klass.name.nil?

  methods = Module.new
  klass.__callbacks.each do |chain, callbacks|
    callbacks.each do |cb|
      f = cb.filter
      case f
      when Symbol
        # One method can be registered on several chains (autosave callbacks are
        # both after_create and after_update), so the label is read from the chain
        # that is running, never from the registration being wrapped.
        next if instrumented.include?([klass, f])

        instrumented << [klass, f]
        methods.define_method(f) do |*args, **kw, &blk|
          ch = $chain_stack.last
          slot = self.class.__callbacks[ch]&.find { |c| c.filter == f }&.kind || :unknown
          journal(self, ch, slot, f.to_s)
          super(*args, **kw, &blk)
        end
      when Proc
        # Named by source location; the proc runs unchanged (it is not wrapped,
        # so it is only journaled through the chain's before/after neighbours).
        nil
      when ActiveModel::Validator
        next if instrumented.include?(f)

        instrumented << f
        name = if f.is_a?(ActiveModel::EachValidator)
                 "validates:#{f.kind}:#{f.attributes.join(',')}"
               else
                 "validates_with:#{f.class.name}"
               end
        f.singleton_class.prepend(Module.new do
          define_method(:validate) do |record|
            journal(record, :validate, :before, name)
            super(record)
          end
        end)
      end
    end
  end
  klass.prepend(methods)
end

# --- effects -----------------------------------------------------------------

TABLES = %w[work_packages journals work_package_journals watchers relations
            custom_values attachments].freeze

def counts
  TABLES.to_h do |t|
    n = ActiveRecord::Base.connection.table_exists?(t) ? ActiveRecord::Base.connection.select_value("SELECT count(*) FROM #{t}").to_i : nil
    [t, n]
  end
end

def delta(before, after)
  TABLES.to_h { |t| [t, before[t] && after[t] ? after[t] - before[t] : nil] }.reject { |_, v| v.nil? || v.zero? }
end

def errors_of(result)
  result.errors.details.to_h { |k, v| [k.to_s, v.map { |d| d.transform_values { |x| x.is_a?(Symbol) ? x.to_s : x.class.name } }] }
end

PERMISSIONS = %i[add_work_packages edit_work_packages view_work_packages manage_subtasks
                 assign_versions change_work_package_status].freeze

def member_user(project, permissions = PERMISSIONS)
  create(:user, member_with_permissions: { project => permissions })
end

def params_for(project, extra = {})
  { subject: "oracle subject", project:, type: project.enabled_types.first,
    status: Status.default || create(:default_status), priority: IssuePriority.default || create(:default_priority) }.merge(extra)
end

CASES = {
  "create_member" => lambda {
    project = create(:project_with_types)
    [member_user(project), "member", params_for(project)]
  },
  "create_no_permission" => lambda {
    project = create(:project_with_types)
    [member_user(project, []), "member_no_permissions", params_for(project)]
  },
  "create_admin" => lambda {
    project = create(:project_with_types)
    [create(:admin), "admin", params_for(project)]
  },
  "create_child_of_automatic_parent" => lambda {
    project = create(:project_with_types)
    parent = create(:work_package, project:, schedule_manually: false)
    [member_user(project), "member", params_for(project, parent:)]
  }
}.freeze

# Each case commits; the next case starts from an empty database so committed
# fixture rows (types, statuses, priorities with fixed names) cannot collide.
KEEP_TABLES = %w[schema_migrations ar_internal_metadata].freeze

def reset_db!
  conn = ActiveRecord::Base.connection
  tables = conn.tables - KEEP_TABLES
  conn.execute("TRUNCATE #{tables.map { |t| conn.quote_table_name(t) }.join(', ')} RESTART IDENTITY CASCADE")
end

abort "refusing to run: work_packages is not empty (use a disposable database)" unless WorkPackage.none?

cases = CASES.map do |name, builder|
  reset_db!
  RequestStore.clear! if defined?(RequestStore)
  User.current = nil
  user, label, params = builder.call
  ActiveJob::Base.queue_adapter.enqueued_jobs.clear

  before = counts
  $journal = []
  result = WorkPackages::CreateService.new(user:).call(**params)
  entries = $journal
  $journal = nil
  after = counts

  wp = result.result
  persisted = wp.respond_to?(:persisted?) && wp.persisted?
  ar_valid = nil
  ar_errors = nil
  if persisted
    fresh = WorkPackage.find(wp.id)
    ar_valid = fresh.valid?
    ar_errors = fresh.errors.details.keys.map(&:to_s).sort
  end

  {
    "name" => name,
    "user" => label,
    "service" => "WorkPackages::CreateService",
    "success" => result.success?,
    "errors" => errors_of(result),
    "persisted" => persisted,
    "changed_attributes" => persisted ? wp.previous_changes.keys.sort : [],
    "row_deltas" => delta(before, after),
    "enqueued_jobs" => ActiveJob::Base.queue_adapter.enqueued_jobs.map { |j| j[:job].name }.sort,
    "ar_valid_after_persist" => ar_valid,
    "ar_errors_after_persist" => ar_errors,
    "journal" => entries.map { |m, phase, cb, depth| { "model" => m, "phase" => phase, "callback" => cb, "tx_depth" => depth } }
  }
end

def sort_keys(o)
  case o
  when Hash then o.sort_by { |k, _| k.to_s }.to_h { |k, v| [k.to_s, sort_keys(v)] }
  when Array then o.map { |v| sort_keys(v) }
  else o
  end
end

trace = sort_keys(
  "source" => { "repo" => "AdaWorldAPI/openproject", "rev" => `git -C #{root} rev-parse HEAD`.strip,
                "ruby" => RUBY_VERSION, "rails" => Rails.version },
  "stubs" => STUBS,
  "journal_limits" => [
    "Proc callbacks run but are not named in the journal",
    "tx_depth is ActiveRecord open_transactions at the callback; 0 means outside any transaction"
  ],
  "cases" => cases
)

out = ENV.fetch("OP_SERVICE_TRACE_OUT") { File.join(__dir__, "work-package-service-trace.json") }
File.write(out, "#{JSON.pretty_generate(trace)}\n")
warn "wrote #{out} (#{cases.size} cases)"
